use bevy::prelude::*;
use mir_project::{
    songs::SongPlugin,
    mic::{MicPlugin, Mic, MicInstruction}
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(MicPlugin)
        .add_plugins(SongPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, print_mir_data)
        .run()
}

fn setup(m: Res<Mic>) {
    println!("{:?}", m.instruction_sender.send(MicInstruction::ConnectToDefaultDevice));
}

fn print_mir_data(m: Res<Mic>) {
    while let Ok(d) = m.data_receiver.try_recv() {
        println!("{:?}", d);
    }
}