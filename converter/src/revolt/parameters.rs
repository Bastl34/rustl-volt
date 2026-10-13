// Parameters.txt - car setup as text: KEY values, blocks like WHEEL 0 { ... }, ; starts a comment

use std::path::Path;

use anyhow::Result;

#[derive(Default)]
pub struct Block
{
    // BODY, WHEEL, SPRING, ... - empty for the outer block
    pub name: String,
    pub index: Option<i32>,
    // keys upper case, values split at whitespace and commas, quotes removed
    pub entries: Vec<(String, Vec<String>)>,
}

#[derive(Default)]
pub struct Parameters
{
    pub top: Block,
    pub blocks: Vec<Block>,
}

impl Block
{
    pub fn values(&self, key: &str) -> Option<&[String]>
    {
        self.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_slice())
    }

    pub fn text(&self, key: &str) -> Option<&str>
    {
        self.values(key)?.first().map(|s| s.as_str())
    }

    pub fn floats(&self, key: &str) -> Vec<f32>
    {
        self.values(key).unwrap_or_default().iter().filter_map(|v| v.parse().ok()).collect()
    }

    pub fn float(&self, key: &str) -> Option<f32>
    {
        self.floats(key).first().copied()
    }

    pub fn vec3(&self, key: &str) -> Option<[f32; 3]>
    {
        let v = self.floats(key);
        (v.len() >= 3).then(|| [v[0], v[1], v[2]])
    }

    pub fn flag(&self, key: &str) -> Option<bool>
    {
        self.text(key).map(|v| v.eq_ignore_ascii_case("true"))
    }
}

impl Parameters
{
    pub fn block(&self, name: &str, index: Option<i32>) -> Option<&Block>
    {
        self.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name) && b.index == index)
    }

    pub fn blocks<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Block> + 'a
    {
        self.blocks.iter().filter(move |b| b.name.eq_ignore_ascii_case(name))
    }

    // MODEL n "cars\rc\body.prm" - "NONE" left out, paths with forward slashes
    pub fn model(&self, index: i32) -> Option<String>
    {
        self.top.entries.iter()
            .filter(|(k, _)| k == "MODEL")
            .find(|(_, v)| v.first().and_then(|n| n.parse::<i32>().ok()) == Some(index))
            .and_then(|(_, v)| v.get(1))
            .filter(|path| !path.eq_ignore_ascii_case("none"))
            .map(|path| path.replace('\\', "/"))
    }
}

pub fn load(path: &Path) -> Result<Parameters>
{
    // not utf-8 everywhere
    let text = String::from_utf8_lossy(&std::fs::read(path)?).to_string();
    let mut parameters = Parameters::default();
    let mut current: Option<Block> = None;

    for line in text.lines()
    {
        let line = line.split(';').next().unwrap_or_default().trim();
        if line.is_empty() || line == "{"
        {
            continue;
        }
        if line.starts_with('}')
        {
            match current.take()
            {
                Some(block) => parameters.blocks.push(block),
                // the outer block is closed - stock cars have a checksum behind it
                None => break,
            }
            continue;
        }

        let opens = line.ends_with('{');
        let line = line.trim_end_matches('{').trim();
        let words = split(line);
        let Some(first) = words.first() else { continue; };

        if opens
        {
            let index = words.get(1).and_then(|w| w.parse().ok());
            current = Some(Block { name: first.to_uppercase(), index, entries: vec![] });
            continue;
        }

        let block = current.as_mut().unwrap_or(&mut parameters.top);
        // a line of numbers only continues the last key (Inertia has 3 rows)
        if first.parse::<f32>().is_ok()
        {
            if let Some((_, values)) = block.entries.last_mut() { values.extend(words); }
            continue;
        }
        block.entries.push((first.to_uppercase(), words[1..].to_vec()));
    }

    Ok(parameters)
}

// at whitespace and commas, "quoted strings" as one word
fn split(line: &str) -> Vec<String>
{
    let mut words = vec![];
    let mut word = String::new();
    let mut quoted = false;

    for c in line.chars()
    {
        match c
        {
            '"' => quoted = !quoted,
            c if !quoted && (c.is_whitespace() || c == ',') =>
            {
                if !word.is_empty() { words.push(std::mem::take(&mut word)); }
            },
            c => word.push(c),
        }
    }
    if !word.is_empty() { words.push(word); }
    words
}
