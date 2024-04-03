
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, BuildStreamError, DefaultStreamConfigError, Device, SampleFormat, Stream, StreamConfig};
use bevy::prelude::*;
use rustfft::{num_complex::{Complex, ComplexFloat}, FftPlanner};
use std::{fmt::Debug, mem::swap, time::Duration};
use crossbeam_channel::{unbounded, Receiver, Sender};

pub const WINDOW_SIZE: usize = 4096; //2048; //8192;

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
    pub mir_receiver: Option<Receiver<FFTInfo>>,
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
    DeviceConnected(Device, Sender<MIRIntruction>, Receiver<FFTInfo>),
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
    ListenFor,
}

pub struct FFTInfo {
    pub data: Vec<f32>,
    pub progress: Duration,
    pub srate: f32,
    pub rms: f32
}

impl FFTInfo {
    pub fn amplitude_at(&self, pitch: f32) -> f32 {
        let continuous_bin = pitch / self.srate * WINDOW_SIZE as f32;
        let left_bin = continuous_bin.floor() as usize;
        let t = continuous_bin % 1.0;
        self.data[left_bin]*(1.0-t) + self.data[left_bin+1]*t
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
fn new_stream(device: &Device, config: &StreamConfig, sample_format: SampleFormat) -> Result<(Stream, Sender<MIRIntruction>, Receiver<FFTInfo>), BuildStreamError> {
    let e = move |err| error!("an error occurred on stream: {}", err);

    let (mir_instruction_sender, mir_instruction_receiver) = unbounded();
    let (mir_response_sender, mir_response_receiver) = unbounded();

    let mut buffer = Vec::new();
    let mut buffer_next = Vec::with_capacity(2*WINDOW_SIZE);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(WINDOW_SIZE);

    let mut song_start = None;

    let srate = config.sample_rate.0 as f32;

    println!("{:?}", config);
    match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(config, move |data: &[f32], callback_info| {

            match mir_instruction_receiver.try_recv() {
                Ok(MIRIntruction::ListenFor) => todo!(),
                Ok(MIRIntruction::SongStart) => {
                    song_start = None;
                    buffer.drain(..);
                    buffer_next.drain(..);
                },
                Err(_) => {},
            }

            if song_start == None {
                song_start = Some(callback_info.timestamp().capture);
                return;
            }
            let song_start = song_start.unwrap();
            let capture = callback_info.timestamp().capture.duration_since(&song_start).unwrap();
            
            if data.len() + buffer.len() < WINDOW_SIZE {
                buffer.extend(data.iter().map(to_complex));
            }
            else {
                let mid = data.len().saturating_sub((data.len() + buffer.len()) % WINDOW_SIZE);
                buffer      .extend(data[..mid].iter().map(to_complex));
                buffer_next .extend(data[mid..].iter().map(to_complex));
            }

            let sections = buffer.len() / WINDOW_SIZE;

            // println!("{:?}; {:?}; {:?}", sections, buffer.len(), buffer_next.len());

            if sections == 0 { return; }

            let start = capture.saturating_sub(Duration::from_secs_f32((buffer.len() + buffer_next.len()) as f32 / srate));
            
            for w in (0..sections).map(|n| n*WINDOW_SIZE) {
                
                let buffer = &mut buffer[w..w+WINDOW_SIZE];

                let rms = (buffer.iter().map(|c| c.re*c.re).sum::<f32>() / WINDOW_SIZE as f32).sqrt();
                
                fft.process(buffer);

                let fft_result: Vec<f32> = buffer.iter().map(|c| c.abs()).collect();

                let progress = start + Duration::from_secs_f32(w as f32 / srate);

                let _ = mir_response_sender.send(FFTInfo {
                    data: fft_result, 
                    rms,
                    progress, 
                    srate
                });
            }

            buffer.drain(..);
            swap(&mut buffer, &mut buffer_next);

        }, e, None),
        _ => todo!()
    }.map(|s| (s, mir_instruction_sender, mir_response_receiver))
}

fn to_complex(v: &f32) -> Complex<f32> {
    Complex {re: *v, im: 0.0}
}