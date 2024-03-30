use bevy::ecs::schedule::States;

pub mod mic;
pub mod settings;
pub mod songs;
pub mod game;

pub const WIDTH: f32 = 1000.0;
pub const HEIGHT: f32 = 600.0;

#[derive(States, Debug, Hash, Clone, PartialEq, Eq, Default)]
pub enum GameState {
    #[default]
    Settings,
    SongLoading,
    SongPlaying,
    PostSongInfo
}