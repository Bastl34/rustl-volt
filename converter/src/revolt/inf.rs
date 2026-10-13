// .inf - level info as text: KEY value ;comment

use std::path::Path;

pub struct Inf
{
    entries: Vec<(String, String)>,
}

impl Inf
{
    pub fn load(path: &Path) -> Self
    {
        let text = std::fs::read(path).map(|data| String::from_utf8_lossy(&data).to_string()).unwrap_or_default();

        let entries = text.lines()
            .map(|line| line.split(';').next().unwrap_or("").trim())
            .filter_map(|line| line.split_once(char::is_whitespace))
            .map(|(key, value)| (key.to_uppercase(), value.trim().to_string()))
            .collect();

        Self { entries }
    }

    pub fn entries(&self) -> &[(String, String)]
    {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&str>
    {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    // NAME 'Supermarket'
    pub fn name(&self) -> Option<String>
    {
        self.get("NAME").map(|name| name.trim_matches('\'').to_string())
    }
}
