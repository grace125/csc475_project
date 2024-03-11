use bevy::{asset::{AssetLoader, AsyncReadExt}, prelude::*, utils::thiserror::Error};
use serde::Deserialize;

pub struct SongPlugin;

impl Plugin for SongPlugin {
    fn build(&self, app: &mut App) {
        app .init_asset::<Song>()
            .register_asset_loader(SongLoader);
    }
}

#[derive(Debug, Deserialize)]
pub enum Tab {
    E2,
    A2,
    D3,
    G3,
    B3,
    E4
}

#[derive(Debug, Deserialize)]
pub struct Note {
    pub tab: Tab,
    pub fret: u32,
    pub beat: f32,
}

impl Note {
    pub fn pitch(&self) -> f32 {
        2.0_f32.powf(1.0/12.0*(self.fret as f32)) * match self.tab {
            Tab::E2 =>  82.41,
            Tab::A2 => 110.00,
            Tab::D3 => 146.83,
            Tab::G3 => 196.00,
            Tab::B3 => 246.94,
            Tab::E4 => 329.63,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SongData {
    pub backing: Option<String>,
    pub bpm: f32,
    pub notes: Vec<Note>
}

#[derive(Asset, TypePath, Debug)]
pub struct Song {
    pub backing: Option<Handle<AudioSource>>,
    pub bpm: f32,
    pub notes: Vec<Note>,
}

#[derive(Default)]
pub struct SongLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SongLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    RonSpannedError(#[from] ron::error::SpannedError),

    #[error(transparent)]
    LoadDirectError(#[from] bevy::asset::LoadDirectError),
}

impl AssetLoader for SongLoader {
    type Asset = Song;

    type Settings = ();

    type Error = SongLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let song_data = ron::de::from_bytes::<SongData>(&bytes)?;
            let backing = song_data.backing.map(|s| load_context.load(&s));
            let song = Song {
                backing,
                bpm: song_data.bpm,
                notes: song_data.notes
            };

            Ok(song)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["song"]
    }
}