
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, BuildStreamError, DefaultStreamConfigError, Device, InputCallbackInfo, SampleFormat, Stream, StreamConfig, StreamInstant};
use bevy::prelude::*;
use rustfft::{num_complex::{Complex, ComplexFloat}, num_traits::Float, Fft, FftPlanner};
use std::{collections::VecDeque, fmt::Debug, sync::Arc, time::Duration};
use crossbeam_channel::{unbounded, Receiver, Sender};

pub const WINDOW_SIZE: usize = 8192;
pub const HOP_SIZE: usize = 2048;
pub const PITCH_APPROXIMATION: f32 = 1.005792941; // 10 cents //1.0116194403; // 20 cents 

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup);
    }
}

// TODO: encapsulate these fields?
#[derive(Resource)]
pub struct Mic {
    pub mir_sender: Option<Sender<MIRIntruction>>,
    pub mir_receiver: Option<Receiver<MagnitudeSpectrum>>,
    pub device_receiver: Receiver<DeviceResponse>,
    pub device_sender: Sender<DeviceInstruction>,
}

pub enum DeviceInstruction {
    GetDevices,
    ConnectToDevice(Device),
    ConnectToDefaultDevice,
    DisconnectFromDevice,
}

impl Debug for DeviceInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GetDevices => write!(f, "GetDevices"),
            Self::ConnectToDevice(_) => f.debug_tuple("ConnectToDevice").field(&"Debug").finish(),
            Self::ConnectToDefaultDevice => write!(f, "ConnectToDefaultDevice"),
            Self::DisconnectFromDevice => write!(f, "DisconnectFromDevice"),
        }
    }
}

pub enum DeviceResponse {
    Devices(Vec<Device>),
    DeviceConnected(Device, Sender<MIRIntruction>, Receiver<MagnitudeSpectrum>),
    DeviceFailedToConnect(MicConnectionError),
    DeviceDisconnected,
}

// TODO: redo this Debug
impl Debug for DeviceResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Devices(_) => f.debug_tuple("Devices").field(&"Debug").finish(),
            Self::DeviceConnected(..) => write!(f, "DeviceConnected"),
            Self::DeviceFailedToConnect(_) => f.debug_tuple("DeviceFailedToConnect").field(&"Err").finish(),
            Self::DeviceDisconnected => write!(f, "DeviceDisconnected"),
        }
    }
}

pub enum MIRIntruction {
    SongStart,
}

pub struct MagnitudeSpectrum {
    pub data: Vec<f32>,
    pub progress: Duration,
    pub srate: f32,
    pub rms: f32
}

impl MagnitudeSpectrum {
    pub fn amplitude_at(&self, pitch: f32) -> f32 {
        let continuous_bin = pitch / self.srate * WINDOW_SIZE as f32;
        let left_bin = (continuous_bin.floor() as usize) % self.data.len();
        let right_bin = (left_bin + 1) % self.data.len();
        let t = continuous_bin % 1.0;
        self.data[left_bin]*(1.0-t) + self.data[right_bin]*t
    }

    pub fn approx_amplitude_at(&self, pitch: f32) -> f32 {
        let min_pitch = pitch / PITCH_APPROXIMATION;
        let max_pitch = pitch * PITCH_APPROXIMATION;

        let left = (min_pitch / self.srate * WINDOW_SIZE as f32) as usize;
        let right = (max_pitch / self.srate * WINDOW_SIZE as f32).ceil() as usize;

        let max = (left..=right).into_iter()
            .map(|i| self.data[i % self.data.len()])
            .reduce(|a, b| if a > b { a } else { b })
            .unwrap();
        max
    }
}

pub enum MicConnectionError {
    DefaultDeviceNotFound,
    ConfigError(Device, DefaultStreamConfigError),
    BuildStreamError(Device, BuildStreamError)
}

fn setup(mut commands: Commands) {
    let (response_sender, response_receiver) = unbounded();
    let (instruction_sender, instruction_receiver) = unbounded();

    std::thread::spawn(move || {
        
        let host = cpal::default_host();

        let mut data = None;

        while let Ok(instruction) = instruction_receiver.recv() {
            match instruction {
                DeviceInstruction::GetDevices => {
                    let _ = response_sender.send(DeviceResponse::Devices(host.input_devices().unwrap().collect()));
                },
                DeviceInstruction::ConnectToDevice(dev) => {
                    try_device_disconnect(&response_sender, &mut data);
                    try_device_connect(response_sender.clone(), &mut data, dev);
                },
                DeviceInstruction::ConnectToDefaultDevice => {
                    try_device_disconnect(&response_sender.clone(), &mut data);
                    let Some(dev) = host.default_input_device() else {
                        let _ = response_sender.send(DeviceResponse::DeviceFailedToConnect(MicConnectionError::DefaultDeviceNotFound));
                        continue
                    };
                    try_device_connect(response_sender.clone(), &mut data, dev);
                },
                DeviceInstruction::DisconnectFromDevice => try_device_disconnect(&response_sender, &mut data),
            }
        }
    });

    commands.insert_resource(Mic { 
        device_sender: instruction_sender, 
        device_receiver: response_receiver,
        mir_sender: None,
        mir_receiver: None 
    });
}

#[inline]
fn try_device_disconnect(response_sender: &Sender<DeviceResponse>, data: &mut Option<(StreamConfig, Stream)>) {
    if data.is_some() {
        let _ = response_sender.send(DeviceResponse::DeviceDisconnected);
    }
    *data = None;
}

#[inline]
fn try_device_connect(response_sender: Sender<DeviceResponse>, data: &mut Option<(StreamConfig, Stream)>, dev: Device) {
    let supported_conf = match dev.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = response_sender.send(DeviceResponse::DeviceFailedToConnect(MicConnectionError::ConfigError(dev, e)));
            return
        }
    };

    let conf = StreamConfig {
        channels: supported_conf.channels(),
        sample_rate: supported_conf.sample_rate(),
        buffer_size: match supported_conf.buffer_size() {
            cpal::SupportedBufferSize::Range { min, .. } => cpal::BufferSize::Fixed(*min.max(&1920)),
            cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Fixed(1920),
        }
    };

    let supported_conf = supported_conf.sample_format();

    let (stream, mir_sender, mir_receiver) = match new_stream(&dev, &conf, supported_conf) {
        Ok(s) => s,
        Err(e) => {
            let _ = response_sender.send(DeviceResponse::DeviceFailedToConnect(MicConnectionError::BuildStreamError(dev, e)));
            return
        }
    };
    stream.play().unwrap();

    *data = Some((conf, stream));

    let _ = response_sender.send(DeviceResponse::DeviceConnected(dev, mir_sender, mir_receiver));
}

#[inline]
fn new_stream(device: &Device, config: &StreamConfig, sample_format: SampleFormat) -> Result<(Stream, Sender<MIRIntruction>, Receiver<MagnitudeSpectrum>), BuildStreamError> {
    let e = move |err| error!("an error occurred on stream: {}", err);

    let (mir_instruction_sender, mir_instruction_receiver) = unbounded();
    let (mir_response_sender, mir_response_receiver) = unbounded();

    let mut buffer: Vec<Complex<f32>> = Vec::with_capacity(WINDOW_SIZE);
    let mut pre_buffer: VecDeque<f32> = VecDeque::with_capacity(2*WINDOW_SIZE);

    let hann = (0..WINDOW_SIZE).into_iter().map(|x| hann(x as f32, WINDOW_SIZE as f32)).collect::<Vec<_>>();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(WINDOW_SIZE);

    let mut song_start = None;

    let srate = config.sample_rate.0 as f32;

    println!("{:?}", config);
    match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(config, move |data: &[f32], callback_info| {

            handle_instructions(&mir_instruction_receiver, &mut song_start, &mut buffer, &mut pre_buffer, callback_info);

            pre_buffer.extend(data);

            let start_progress = start_to_capture(&song_start, &callback_info).saturating_sub(Duration::from_secs_f32(pre_buffer.len() as f32 / srate));

            let mut h = 0;
            while h + WINDOW_SIZE < pre_buffer.len() {
                let progress = start_progress + Duration::from_secs_f32(h as f32 / srate);
                buffer.extend(pre_buffer.iter().skip(h).take(WINDOW_SIZE).map(to_complex));

                let rms = buffer.iter().map(|c| c.re*c.re).sum::<f32>() / WINDOW_SIZE as f32;

                let fft_result = calculate_spectrogram(&fft, &hann, &mut buffer[0..WINDOW_SIZE]);

                let _ = mir_response_sender.send(MagnitudeSpectrum {
                    data: fft_result, 
                    rms,
                    progress, 
                    srate
                });
                
                buffer.drain(..);
                h += HOP_SIZE;
            }

            pre_buffer.drain(..h);

        }, e, None),
        _ => todo!()
    }.map(|s| (s, mir_instruction_sender, mir_response_receiver))
}

#[inline]
fn handle_instructions(
    mir_instruction_receiver: &Receiver<MIRIntruction>, 
    song_start: &mut Option<StreamInstant>, 
    buffer: &mut Vec<Complex<f32>>,
    pre_buffer: &mut VecDeque<f32>,
    callback_info: &InputCallbackInfo
) {
    match mir_instruction_receiver.try_recv() {
        Ok(MIRIntruction::SongStart) => {
            *song_start = None;
            buffer.drain(..);
            pre_buffer.drain(..);
        },
        Err(_) => {},
    }

    if *song_start == None {
        *song_start = Some(callback_info.timestamp().capture);
    }
}

#[inline]
fn calculate_spectrogram(fft: &Arc<dyn Fft<f32>>, window_func: &Vec<f32>, buffer: &mut [Complex<f32>]) -> Vec<f32> {
    for (i, c) in buffer.iter_mut().enumerate() {
        *c *= to_complex(&window_func[i])
    }
    fft.process(buffer);
    buffer.iter().map(|c| c.abs()).collect()
}

#[inline]
fn hann(x: f32, m: f32) -> f32 {
    (1.0 + (std::f32::consts::TAU*x/m + std::f32::consts::PI).cos())/2.0
}

#[inline]
fn start_to_capture(
    song_start: &Option<StreamInstant>, 
    callback_info: &InputCallbackInfo
) -> Duration {
    callback_info.timestamp().capture.duration_since(&song_start.unwrap()).unwrap()
}

#[inline]
fn to_complex(v: &f32) -> Complex<f32> {
    Complex {re: *v, im: 0.0}
}