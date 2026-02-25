use std::marker::PhantomData;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RonLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON parse error: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

#[derive(TypePath)]
pub struct RonLoader<T: TypePath> {
    extensions: Vec<&'static str>,
    _phantom: PhantomData<T>,
}

impl<T: TypePath> RonLoader<T> {
    pub fn new(extensions: &[&'static str]) -> Self {
        Self {
            extensions: extensions.to_vec(),
            _phantom: PhantomData,
        }
    }
}

impl<T> AssetLoader for RonLoader<T>
where
    T: Asset + TypePath + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    type Asset = T;
    type Settings = ();
    type Error = RonLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let asset = ron::de::from_bytes::<T>(&bytes)?;
        Ok(asset)
    }

    fn extensions(&self) -> &[&str] {
        &self.extensions
    }
}
