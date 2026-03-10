use bevy::prelude::*;
use serde::Deserialize;

#[derive(Component, Debug)]
pub struct Trader;

#[derive(Component, Debug, Clone, Deserialize)]
pub struct TradeOffers {
    pub offers: Vec<TradeOffer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeOffer {
    pub cost: Vec<(String, u16)>,
    pub result: (String, u16),
}
