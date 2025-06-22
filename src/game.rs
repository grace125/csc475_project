use bevy::{prelude::*, time::Stopwatch};
use bevy_egui::{egui, EguiContexts};

use crate::{mic::{MIRIntruction, MagnitudeSpectrum, Mic}, songs::{Note, Song, Tab}, GameState, HEIGHT, WIDTH};


pub const NOTE_RADIUS: f32 = 25.0;

pub const NOTE_COLOR: Color = Color::GOLD;
pub const NOTE_FONT_SIZE: f32 = 30.0;
pub const HIT_Y_POS: f32 = HEIGHT/4.0;
pub const SPAWN_Y_POS: f32 = -(HEIGHT/2.0) - NOTE_RADIUS;
pub const DESPAWN_Y_POS: f32 = -SPAWN_Y_POS;
pub const SCROLL_TIME: f32 = 3.25;
pub const COLUMN_SPACE: f32 = WIDTH/7.0;

pub const SCORE_THRESHOLD: f32 = 40.0;

pub const HIT_FORGIVENESS: f32 = 0.20;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app .add_systems(OnExit(GameState::SongPlaying), (despawn_all::<Note>, despawn_all::<Backing>))
            .add_systems(Update, (update_stopwatch, rhythm_calculator, note_animator, display_game).chain().run_if(in_state(GameState::SongPlaying)))
            .add_systems(Update, post_game_info.run_if(in_state(GameState::PostSongInfo)));
    }
}

#[derive(Component)]
pub struct Backing;

#[derive(Resource)]
pub struct CurrentSong {
    latest_unplayed_note: usize,
    pub asset: Handle<Song>,
    pub speed: f32,
    stopwatch: Stopwatch,
    success: usize,
}

impl CurrentSong {
    pub fn new(asset: Handle<Song>, speed: f32) -> Self {
        CurrentSong {
            asset,
            stopwatch: Stopwatch::new(),
            latest_unplayed_note: 0,
            success: 0,
            speed,
        }
    }
}

#[derive(Default, Component)]
pub struct NoteHitData {
    pub data: Vec<(f32, f32)>
}

fn despawn_all<T: Component>(mut commands: Commands, notes: Query<Entity, With<T>>) {
    for e in notes.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn update_stopwatch(
    mut commands: Commands,
    time: Res<Time>,
    songs: Res<Assets<Song>>,
    mut song_data: ResMut<CurrentSong>,
    mic: Res<Mic>,
) {
    let prev_time = song_data.stopwatch.elapsed_secs();
    song_data.stopwatch.tick(time.delta());
    let this_time = song_data.stopwatch.elapsed_secs();
    
    if prev_time <= 0.0 && 0.0 <= this_time {

        if let Some(sender) = &mic.mir_sender {
            let _ = sender.send(MIRIntruction::SongStart);
        }

        let song = songs.get(&song_data.asset).unwrap();
        if let Some(backing) = &song.backing {
            commands.spawn((
                AudioSourceBundle {
                    source: backing.clone_weak(),
                    settings: PlaybackSettings::DESPAWN.with_speed(song_data.speed),
                }, 
                Backing
            ));
        }
    }
}

#[inline]
fn tab_to_column(tab: Tab) -> f32 {
    -(WIDTH/2.0) + COLUMN_SPACE * match tab {
        Tab::E2 => 1.0,
        Tab::A2 => 2.0,
        Tab::D3 => 3.0,
        Tab::G3 => 4.0,
        Tab::B3 => 5.0,
        Tab::E4 => 6.0,
    }
}

fn note_animator(
    mut commands: Commands,
    mut notes: Query<(Entity, &Note, &mut Transform)>,
    mut song_data: ResMut<CurrentSong>,
    mut next_state: ResMut<NextState<GameState>>,
    songs: Res<Assets<Song>>,
) {
    let song = songs.get(&song_data.asset).unwrap();
    
    let elapsed_time = song_data.stopwatch.elapsed_secs() * song_data.speed;
    let bps = song.bpm / 60.0;

    if song_data.latest_unplayed_note >= song.notes.len() && notes.is_empty() {
        next_state.set(GameState::PostSongInfo);
        return;
    }

    for (e, note, mut transform) in notes.iter_mut() {
        let hit_time = note.beat / bps;
        let spawn_time = hit_time - SCROLL_TIME;

        let p = (elapsed_time - spawn_time) / SCROLL_TIME;

        let new_y_pos = SPAWN_Y_POS*(1.0 - p) + HIT_Y_POS*p;

        if new_y_pos > DESPAWN_Y_POS {
            commands.entity(e).despawn();
        }
        else {
            transform.translation.y = new_y_pos;
        }
    }

    loop {
        let Some(note) = song.notes.get(song_data.latest_unplayed_note) else { 
            break;
        };

        let hit_time = note.beat / bps;
        let spawn_time = hit_time - SCROLL_TIME;
        let p = (elapsed_time - spawn_time) / SCROLL_TIME;

        if p > 0.0 {
            let y = SPAWN_Y_POS*(1.0 - p) + HIT_Y_POS*p;
            let x = tab_to_column(note.tab);

            commands.spawn((
                (*note).clone(),
                NoteHitData::default(),
                TransformBundle {
                    local: Transform::from_xyz(x, y, 0.0),
                    ..default()
                },
                VisibilityBundle::default()
            )).with_children(|parent| {
                parent.spawn(Text2dBundle {
                    text: Text::from_section(
                        format!("{:?}", note.fret), 
                        TextStyle {
                            font_size: NOTE_FONT_SIZE,
                            color: NOTE_COLOR,
                            ..default()
                        }),
                    // text_2d_bounds: todo!(),
                    // transform: todo!(),
                    // global_transform: todo!(),
                    // visibility: todo!(),
                    // inherited_visibility: todo!(),
                    // view_visibility: todo!(),
                    // text_layout_info: todo!(),
                    ..default()
                });
            });
            song_data.latest_unplayed_note += 1;
        }
        else {
            break;
        }
    }
}

fn rhythm_calculator(
    mut commands: Commands,
    mic: Res<Mic>,
    mut notes: Query<(Entity, &Note, &mut NoteHitData)>,
    mut song_data: ResMut<CurrentSong>,
    songs: Res<Assets<Song>>,
) {
    let bps = songs.get(&song_data.asset).unwrap().bpm / 60.0;

    if let Some(mir_receiver) = &mic.mir_receiver {
        while let Ok(fft_info) = mir_receiver.try_recv() {
            for (e, note, mut note_hit_data) in notes.iter_mut() {
                let note_time = note.beat / bps * song_data.speed; 

                let diff = fft_info.progress.as_secs_f32() - note_time;
                
                if diff > HIT_FORGIVENESS {
                    commands.entity(e).remove::<NoteHitData>();

                    // println!("\nScores for note {:?}", note);
                    for (diff, score) in note_hit_data.data.iter() {
                        println!("{:.0} at diff {:.6}", score.floor(), *diff);
                        if *score > SCORE_THRESHOLD {
                            println!("Note {:?} Hit!", note);
                            commands.entity(e).despawn_recursive();
                            song_data.success += 1;
                            break;
                        }
                    }

                    // println!("\nScore differences :");
                    // for (d, s) in note_hit_data.data.windows(2).map(|slice| (slice[0].0, slice[0].1 - slice[1].1)) {
                    //     println!("{:.0} at diff {:.6}", s.floor(), d); 
                    // }

                    continue;
                }
                else if diff < -HIT_FORGIVENESS {
                    continue;
                }

                let score = calculate_score(note.pitch(), &fft_info);


                note_hit_data.data.push((diff, score));

            }
        }
    }
}

pub fn calculate_score(pitch: f32, spectrum: &MagnitudeSpectrum) -> f32 {
    let first_harm = pitch;
    let second_harm = 2.0*first_harm;
    let third_harm = 3.0*first_harm;

    // let score = (
    //     spectrum.amplitude_at(first_harm).powi(4)
    //     * spectrum.amplitude_at(second_harm).powi(2)
    //     * spectrum.amplitude_at(third_harm)
    // ).powf(1.0/7.0);

    // let score = (
    //     spectrum.approx_amplitude_at(first_harm).powi(2)
    //     * spectrum.approx_amplitude_at(second_harm)
    //     * spectrum.approx_amplitude_at(third_harm)
    // ).powf(0.25);

    // let score = (
    //     4.0*spectrum.amplitude_at(first_harm)
    //     + 2.0*spectrum.amplitude_at(second_harm)
    //     + spectrum.amplitude_at(third_harm)
    // ) / 7.0;

    let score = spectrum.approx_amplitude_at(first_harm);
    
    // let score = spectrum.amplitude_at(first_harm);
    // let score = score / spectrum.mean_squared.sqrt(); //.max(0.05);
    // let score = score / spectrum.rms.sqrt().max(0.05);

    score
}

fn display_game(mut gizmos: Gizmos, notes: Query<&mut Transform, With<Note>>) {

    let columns = [tab_to_column(Tab::E2), tab_to_column(Tab::A2), tab_to_column(Tab::D3), tab_to_column(Tab::G3), tab_to_column(Tab::B3), tab_to_column(Tab::E4)];

    for note in notes.iter() {
        gizmos.circle_2d(note.translation.xy(), NOTE_RADIUS, NOTE_COLOR);
    }

    for column in columns.iter() {
        gizmos.circle_2d(Vec2::new(*column , HIT_Y_POS), NOTE_RADIUS, Color::WHITE);
    }
}


fn post_game_info(
    mut contexts: EguiContexts,
    mut next_state: ResMut<NextState<GameState>>,
    song_data: Res<CurrentSong>,
    songs: Res<Assets<Song>>,
) {

    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {

        ui.heading("Stats");
        ui.separator();

        ui.label(format!("Notes hit: {}", song_data.success));

        let total_notes = songs.get(&song_data.asset).unwrap().notes.len();
        ui.label(format!("Total notes: {}", total_notes));

        ui.separator();
        if ui.button("Main Menu").clicked() {
            next_state.set(GameState::Settings);
        }

    });
}