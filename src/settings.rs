use bevy:: prelude::*;
use bevy_egui::{egui, EguiContexts};
use cpal::{traits::DeviceTrait, Device};

use crate::{game::CurrentSong, mic::{Mic, MicInstruction, MicResponse}, GameState};

pub struct SettingsUiPlugin;

impl Plugin for SettingsUiPlugin {
    fn build(&self, app: &mut App) {
        app .init_resource::<AvailableDevices>()
            .add_systems(Startup, get_devices)
            .add_systems(Update, (mic_response_handler, settings).chain().run_if(in_state(GameState::Settings)))
            .add_systems(Update, loading.run_if(in_state(GameState::SongLoading)));
    }
}

#[derive(Resource, Default)]
pub struct AvailableDevices {
    pub available: Vec<Device>,
    pub connected: Option<Device>
}

#[derive(PartialEq)]
enum SelectedSong { 
    TwinkleTwinkle, 
    SoundOfSilence,
    TestSong,
}

fn get_devices(mic: Res<Mic>) {
    let _ = mic.instruction_sender.send(MicInstruction::GetDevices);
}

fn mic_response_handler(
    mic: Res<Mic>,
    mut available_devices: ResMut<AvailableDevices>
) {
    // TODO: handle this later
    while let Ok(response) = mic.response_receiver.try_recv() {
        match response {
            MicResponse::Devices(devices) => {
                available_devices.available = devices;
            },
            MicResponse::DeviceConnected(dev) => {
                available_devices.connected = Some(dev);
            },
            MicResponse::DeviceDisconnected => {
                available_devices.connected = None;
            },
            MicResponse::DeviceFailedToConnect(_) => (), // error!("Failed to connect to device: {:?}", e),
        }
    }

}

fn settings(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut selected_song: Local<Option<SelectedSong>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut devices: ResMut<AvailableDevices>,
    mic: Res<Mic>,
) {
    let ctx = contexts.ctx_mut();
    
    egui::SidePanel::left("side_panel").default_width(500.0).show(ctx, |ui| {
        ui.heading("Select Mic");
        ui.separator();

        if let Some(connected_device) = &devices.connected {
            ui.label(format!("Connected device: {:?}", connected_device.name()));
            if ui.button("Disconnect").clicked() {
                let _ = mic.instruction_sender.send(MicInstruction::DisconnectFromDevice);
            }
            ui.separator();
        }

        for index in 0..devices.available.len() {
            let device = devices.available.get(index).unwrap();
            let name = match device.name() {
                Ok(name) => name,
                Err(_) => format!("{:?}", index)
            };
            if ui.button(name).clicked() {
                let device = devices.available.remove(index);
                let _ = mic.instruction_sender.send(MicInstruction::ConnectToDevice(device));
                let _ = mic.instruction_sender.send(MicInstruction::GetDevices);
            }
        }

        ui.separator();

        if ui.button("Refresh").clicked() {
            let _ = mic.instruction_sender.send(MicInstruction::GetDevices);
        }
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Select Song");
        ui.separator();

        let selected_song = &mut *selected_song;

        ui.selectable_value(selected_song, Some(SelectedSong::TestSong), "Test");
        ui.selectable_value(selected_song, Some(SelectedSong::TwinkleTwinkle), "Twinkle Twinkle Little Star");
        ui.selectable_value(selected_song, Some(SelectedSong::SoundOfSilence), "Sound of Silence");

        
        if ui.add_enabled(selected_song.is_some() && devices.connected.is_some(), egui::Button::new("Play")).clicked() {
            commands.insert_resource(CurrentSong::new(
                match selected_song {
                    Some(SelectedSong::TestSong)        => asset_server.load("songs/test.song"),
                    Some(SelectedSong::TwinkleTwinkle)  => asset_server.load("songs/twinkle-twinkle.song"),
                    Some(SelectedSong::SoundOfSilence)  => asset_server.load("songs/sound-of-silence.song"),
                    None => unreachable!()
                }
            ));
            next_state.set(GameState::SongLoading);
        }
    });
}

fn loading(
    mut next_state: ResMut<NextState<GameState>>,
    asset_server: Res<AssetServer>,
    song: Res<CurrentSong>,
) {
    if asset_server.is_loaded_with_dependencies(&song.asset) {
        next_state.set(GameState::SongPlaying);
    }
}







// ui.hyperlink("https://github.com/emilk/egui_template");
        // ui.add(egui::github_link_file_line!(
        //     "https://github.com/mvlabat/bevy_egui/blob/main/",
        //     "Direct link to source code."
        // ));
        // egui::warn_if_debug_build(ui);

        // ui.separator();

        // ui.heading("Central Panel");
        // ui.label("The central panel is the region left after adding TopPanels and SidePanels.");
        // ui.label("It is often a great place for big things, like drawings:");

        // ui.heading("Draw with your mouse to paint:");
        // ui_state.painting.ui_control(ui);
        // egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
        //     ui_state.painting.ui_content(ui);
        // });