use std::borrow::Cow;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config<'a> {
    pub host: Cow<'a, str>,
    pub token: Cow<'a, str>,
    pub namespace: Vec<Cow<'a, str>>,
}
