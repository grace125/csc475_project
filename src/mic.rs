
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, BuildStreamError, DefaultStreamConfigError, Device, DevicesError, InputCallbackInfo, Sample, Stream, SupportedStreamConfig};
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
    Devices(Result<Vec<Device>, DevicesError>),
    DeviceConnected,
    DeviceFailedToConnect(MicConnectionError),
    DeviceDisconnected,
}

impl Debug for MicResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Devices(_) => f.debug_tuple("Devices").field(&"Debug").finish(),
            Self::DeviceConnected => write!(f, "DeviceConnected"),
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
                    let _ = response_sender.send(MicResponse::Devices(host.input_devices().map(|devs| devs.collect())));
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
fn try_device_disconnect(response_sender: &Sender<MicResponse>, data: &mut Option<(Device, SupportedStreamConfig, Stream)>) {
    if data.is_some() {
        let _ = response_sender.send(MicResponse::DeviceDisconnected);
    }
    *data = None;
}

#[inline]
fn try_device_connect(response_sender: Sender<MicResponse>, data_sender: Sender<u32>, data: &mut Option<(Device, SupportedStreamConfig, Stream)>, dev: Device) {
    let conf = match dev.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = response_sender.send(MicResponse::DeviceFailedToConnect(MicConnectionError::ConfigError(dev, e)));
            return
        }
    };
    let stream = match new_stream(data_sender, &dev, &conf) {
        Ok(s) => s,
        Err(e) => {
            let _ = response_sender.send(MicResponse::DeviceFailedToConnect(MicConnectionError::BuildStreamError(dev, e)));
            return
        }
    };
    stream.play().unwrap();
    *data = Some((dev, conf, stream));
    let _ = response_sender.send(MicResponse::DeviceConnected);
}

#[inline]
fn new_stream(data_sender: Sender<u32>, device: &Device, config: &SupportedStreamConfig) -> Result<Stream, BuildStreamError> {
    let sample_format = config.sample_format();
    let e = move |err| error!("an error occurred on stream: {}", err);
    match sample_format {
        cpal::SampleFormat::I8 => device.build_input_stream(&config.config(), move |con, callback| process_mic_data::<i8>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::I16 => device.build_input_stream(&config.config(), move |con, callback| process_mic_data::<i16>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::I32 => device.build_input_stream(&config.config(), move |con, callback| process_mic_data::<i32>(&data_sender, con, callback), e, None),
        cpal::SampleFormat::F32 => device.build_input_stream(&config.config(), move |con, callback| process_mic_data::<f32>(&data_sender, con, callback), e, None),
        _ => todo!()
    }
}

fn process_mic_data<T: Sample + Debug>(_data_sender: &Sender<u32>, input: &[T], _input_callback_info: &InputCallbackInfo) {
    println!("mic data: {:?}", input.len());
    // data_sender.send(0);
}