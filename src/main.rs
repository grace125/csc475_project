use bevy::{prelude::*, window::WindowResolution};
use mir_project::{
    game::GamePlugin, mic::MicPlugin, settings::SettingsUiPlugin, songs::SongPlugin, GameState, HEIGHT, WIDTH
};


fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(WIDTH, HEIGHT),
                    title: "Rhythm Game".to_owned(),
                    resizable: false,
                    ..default()
                }),
                ..default()
            }), 
            bevy_egui::EguiPlugin, 
            MicPlugin, 
            SongPlugin, 
            SettingsUiPlugin,
            GamePlugin
        ))
        .add_systems(Startup, setup)
        .init_state::<GameState>()
        .run()
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}