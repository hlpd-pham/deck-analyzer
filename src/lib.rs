use std::{
    fs::File,
    io::{BufRead, BufReader},
};
pub mod db;
pub mod types;
use rusqlite::{Connection, params};
use types::ScryfallCard;
