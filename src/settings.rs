use bevy:: prelude::*;
use bevy_egui::{egui::{self, Color32}, EguiContexts};
use cpal::{traits::DeviceTrait, Device};
use egui_plot::{Line, PlotPoints};

use crate::{game::{calculate_score, CurrentSong, SCORE_THRESHOLD}, mic::{DeviceInstruction, DeviceResponse, MagnitudeSpectrum, Mic, WINDOW_SIZE}, GameState};

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
    let _ = mic.device_sender.send(DeviceInstruction::GetDevices);
}

fn mic_response_handler(
    mut mic: ResMut<Mic>,
    mut available_devices: ResMut<AvailableDevices>
) {
    // TODO: handle this later
    while let Ok(response) = mic.device_receiver.try_recv() {
        match response {
            DeviceResponse::Devices(devices) => {
                available_devices.available = devices;
            },
            DeviceResponse::DeviceConnected(dev, sender, receiver) => {
                mic.mir_sender = Some(sender);
                mic.mir_receiver = Some(receiver);
                available_devices.connected = Some(dev);
            },
            DeviceResponse::DeviceDisconnected => {
                mic.mir_sender = None;
                mic.mir_receiver = None;
                available_devices.connected = None;
            },
            DeviceResponse::DeviceFailedToConnect(_) => (), // error!("Failed to connect to device: {:?}", e),
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
    mut spectrum: Local<Option<MagnitudeSpectrum>>,
) {
    let ctx = contexts.ctx_mut();
    
    egui::SidePanel::left("side_panel").default_width(500.0).show(ctx, |ui| {
        ui.heading("Select Mic");
        ui.separator();

        if let Some(connected_device) = &devices.connected {
            ui.label(format!("Connected device: {:?}", connected_device.name()));
            if ui.button("Disconnect").clicked() {
                let _ = mic.device_sender.send(DeviceInstruction::DisconnectFromDevice);
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
                let _ = mic.device_sender.send(DeviceInstruction::ConnectToDevice(device));
                let _ = mic.device_sender.send(DeviceInstruction::GetDevices);
            }
        }

        ui.separator();

        if ui.button("Refresh").clicked() {
            let _ = mic.device_sender.send(DeviceInstruction::GetDevices);
        }

        if let Some(mir_receiver) = &mic.mir_receiver {
            while let Ok(s) = mir_receiver.try_recv() {
                *spectrum = Some(s);
            }
        }
        else {
            *spectrum = None
        }

        ui.separator();

        if let Some(spectrum) = &*spectrum {
            let spectrogram_line: PlotPoints = spectrum.data[0..spectrum.data.len()/2].iter().enumerate().skip(1).map(|(x, y)| {
                let x = (x as f64 * (spectrum.srate as f64) / WINDOW_SIZE as f64).log2();
                let y = *y as f64;
                [x, y]
            }).collect();
            
            let score_line: Vec<[f64; 2]> = (0..spectrum.data.len()/2).into_iter().skip(1).map(|x| {
                let x = x as f64 * (spectrum.srate as f64)  / WINDOW_SIZE as f64;
                let y = calculate_score(x as f32, &spectrum) as f64;
                let x = x.log2();
                [x, y]
            }).collect();   

            let threshold_line: Vec<[f64; 2]> = vec![[score_line[0][0], SCORE_THRESHOLD as f64], [score_line.last().unwrap()[0], SCORE_THRESHOLD as f64]];

            let spectrogram_line = Line::new(spectrogram_line).color(Color32::from_rgb(255, 0, 0));
            let score_line = Line::new(score_line).color(Color32::from_rgb(0, 0, 255));
            let threshold_line = Line::new(threshold_line).color(Color32::from_rgb(0, 255, 0));
            
            egui_plot::Plot::new("FFT").include_y(200.0).include_y(0.0).view_aspect(2.0).show(ui, |plot_ui| {
                plot_ui.line(spectrogram_line);
            });

            egui_plot::Plot::new("Score").include_y(400.0).include_y(0.0).view_aspect(2.0).show(ui, |plot_ui| {
                plot_ui.line(score_line);
                plot_ui.line(threshold_line);
            });
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