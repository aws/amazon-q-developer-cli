mod error;
pub mod midway;
mod reqwest_client;

pub use error::Error;
pub use reqwest;
use reqwest::Client;
pub use reqwest::{
    Error as ReqwestError,
    Method,
};

pub fn client() -> Option<&'static Client> {
    reqwest_client::reqwest_client()
}

pub fn client_no_redirect() -> Option<&'static Client> {
    reqwest_client::reqwest_client_no_redirect()
}
