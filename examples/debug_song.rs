use std::{env, time::Duration};

use bevy::{prelude::*, time::Stopwatch};
use mir_project::songs::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SongPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, play_song)
        .run();
}

#[derive(Debug, Clone, Resource)]
pub struct DebugSong(Handle<Song>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut args = env::args();
    args.next();
    commands.insert_resource(DebugSong(asset_server.load(args.next().unwrap())));
}

fn play_song(
    mut commands: Commands, 
    mut flag: Local<bool>, 
    mut stopwatch: Local<Stopwatch>,
    mut pitch_assets: ResMut<Assets<Pitch>>,
    time: Res<Time>,
    debug_song: Res<DebugSong>, 
    songs: Res<Assets<Song>>,
) {
    let Some(song) = songs.get(debug_song.0.clone_weak()) else { return };
    if !*flag {
        *flag = true;
        if let Some(backing) = &song.backing {
            commands.spawn(AudioBundle {
                source: backing.clone(),
                settings: PlaybackSettings::DESPAWN
            });
        }
    }
    let prev_time_in_beats = stopwatch.elapsed_secs() / 60.0 * song.bpm;
    stopwatch.tick(time.delta());
    let curr_time_in_beats = stopwatch.elapsed_secs() / 60.0 * song.bpm;

    for note in song.notes.iter() {
        if prev_time_in_beats <= note.beat && note.beat < curr_time_in_beats {
            commands.spawn(PitchBundle {
                source: pitch_assets.add(Pitch::new(note.pitch(), Duration::from_secs_f32(0.25 / 60.0 * song.bpm))),
                settings: PlaybackSettings::DESPAWN
            });
        }
    }
}