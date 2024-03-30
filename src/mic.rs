
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, BuildStreamError, DefaultStreamConfigError, Device, InputCallbackInfo, Sample, SampleFormat, Stream, StreamConfig};
use bevy::prelude::*;
use std::fmt::Debug;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

// TODO: tweak value
// pub const CHANNEL_MAX = 256;

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup);
    }
}

// TODO: encapsulate these fields?
#[derive(Resource)]
pub struct Mic {
    pub data_receiver: Receiver<u32>,
    pub response_receiver: Receiver<MicResponse>,
    pub instruction_sender: Sender<MicInstruction>,
}

pub enum MicInstruction {
    GetDevices,
    ConnectToDevice(Device),
    ConnectToDefaultDevice,
    DisconnectFromDevice,
}

impl Debug for MicInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GetDevices => write!(f, "GetDevices"),
            Self::ConnectToDevice(_) => f.debug_tuple("ConnectToDevice").field(&"Debug").finish(),
            Self::ConnectToDefaultDevice => write!(f, "ConnectToDefaultDevice"),
            Self::DisconnectFromDevice => write!(f, "DisconnectFromDevice"),
        }
    }
}

pub enum MicResponse {
    Devices(Vec<Device>),
    DeviceConnected(Device),
    DeviceFailedToConnect(MicConnectionError),
    DeviceDisconnected,
}

// TODO: redo this Debug
impl Debug for MicResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Devices(_) => f.debug_tuple("Devices").field(&"Debug").finish(),
            Self::DeviceConnected(..) => write!(f, "DeviceConnected"),
            Self::DeviceFailedToConnect(_) => f.debug_tuple("DeviceFailedToConnect").field(&"Err").finish(),
            Self::DeviceDisconnected => write!(f, "DeviceDisconnected"),
        }
    }
}

pub enum MicConnectionError {
    DefaultDeviceNotFound,
    ConfigError(Device, DefaultStreamConfigError),
    BuildStreamError(Device, BuildStreamError)
}

fn setup(mut commands: Commands) {
    let (data_sender, data_receiver) = bounded(256);
    let (response_sender, response_receiver) = unbounded();
    let (instruction_sender, instruction_receiver) = unbounded();

    std::thread::spawn(move || {
        
        let host = cpal::default_host();

        let mut data = None;

        while let Ok(instruction) = instruction_receiver.recv() {
            match instruction {
                MicInstruction::GetDevices => {
                    let _ = response_sender.send(MicResponse::Devices(host.input_devices().unwrap().collect()));
                },
                MicInstruction::ConnectToDevice(dev) => {
                    try_device_disconnect(&response_sender, &mut data);
                    try_device_connect(response_sender.clone(), data_sender.clone(), &mut data, dev);
                },
                MicInstruction::ConnectToDefaultDevice => {
                    try_device_disconnect(&response_sender.clone(), &mut data);
                    let Some(dev) = host.default_input_device() else {
                        let _ = response_sender.send(MicResponse::DeviceFailedToConnect(MicConnectionError::DefaultDeviceNotFound));
                        continue
                    };
                    try_device_connect(response_sender.clone(), data_sender.clone(), &mut data, dev);
                },
                MicInstruction::DisconnectFromDevice => try_device_disconnect(&response_sender, &mut data),
            }
        }
    });

    commands.insert_resource(Mic { data_receiver, instruction_sender, response_receiver });
}

#[inline]
fn try_device_disconnect(response_sender: &Sender<MicResponse>, data: &mut Option<(StreamConfig, Stream)>) {
    if data.is_some() {
        let _ = response_sender.send(MicResponse::DeviceDisconnected);
    }
    *data = None;
}

#[inline]
fn try_device_connect(response_sender: Sender<MicResponse>, data_sender: Sender<u32>, data: &mut Option<(StreamConfig, Stream)>, dev: Device) {
    let supported_conf = match dev.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = response_sender.send(MicResponse::DeviceFailedToConnect(MicConnectionError::ConfigError(dev, e)));
            return
        }
    };

    let conf = StreamConfig {
        channels: supported_conf.channels(),
        sample_rate: supported_conf.sample_rate(),
        buffer_size: match supported_conf.buffer_size() {
            cpal::SupportedBufferSize::Range { min, .. } => cpal::BufferSize::Fixed(*min.max(&256)),
            cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Fixed(256),
        }
    };

    let supported_conf = supported_conf.sample_format();

    let stream = match new_stream(data_sender, &dev, &conf, supported_conf) {
        Ok(s) => s,
        Err(e) => {
            let _ = response_sender.send(MicResponse::DeviceFailedToConnect(MicConnectionError::BuildStreamError(dev, e)));
            return
        }
    };
    stream.play().unwrap();

    *data = Some((conf, stream));

    let _ = response_sender.send(MicResponse::DeviceConnected(dev));
}

#[inline]
fn new_stream(data_sender: Sender<u32>, device: &Device, config: &StreamConfig, sample_format: SampleFormat) -> Result<Stream, BuildStreamError> {
    let e = move |err| error!("an error occurred on stream: {}", err);

    println!("{:?}", config);
    match sample_format {
        cpal::SampleFormat::I8 => device.build_input_stream(config, move |con, callback| process_mic_data::<i8>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::I16 => device.build_input_stream(config, move |con, callback| process_mic_data::<i16>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::I32 => device.build_input_stream(config, move |con, callback| process_mic_data::<i32>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::F32 => device.build_input_stream(config, move |con, callback| process_mic_data::<f32>(&data_sender, con, callback), e, None),
        _ => todo!()
    }
}

fn process_mic_data<T: Sample + Debug>(_data_sender: &Sender<u32>, _input: &[T], _input_callback_info: &InputCallbackInfo) {
    println!("mic data: {:?}", _input_callback_info);//input.len());
    // data_sender.send(0);
}