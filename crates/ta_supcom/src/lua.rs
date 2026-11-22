//! The Lua that Supreme Commander keeps its maps' and props' data in: no program, only values.
//! A reader of that subset, not of Lua: assignments at the top (`ScenarioInfo = { ... }`,
//! `Scenario = { ... }`) or calls (`PropBlueprint { ... }`), whose values are nil, booleans,
//! numbers (`6.75917e-005`), strings in either quote, tables (`name = v`, `['key'] = v`,
//! `[1] = v` and bare values) and the save files' constructors, read as calls: `STRING('...')`,
//! `FLOAT(1)`, `BOOLEAN(true)`, `VECTOR3(x, y, z)`, `RECTANGLE(...)`, `GROUP { ... }`, and
//! a blueprint's `Sound { ... }`. Comments are Lua's (`--`, `--[[ ]]`, `--[==[ ]==]`) and the
//! blueprints' `#`; long strings of any level (`[[ ]]`, `[==[ ]==]`). Nothing is run. Of a
//! map's script, only what it calls is looked for ([`call_string`]).

use crate::{Error, Result};

/// How deep tables may nest (the save files go about eight deep).
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table(Table),
    /// `NAME(args)`, `NAME { table }` or `NAME 'string'`.
    Call(String, Vec<Value>),
}

/// A table: its keyed entries in the file's order, and its bare values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub fields: Vec<(String, Value)>,
    pub array: Vec<Value>,
}

impl Table {
    /// The entry `key` (exactly as written).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The entry `key`, whatever its case.
    pub fn get_ci(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    /// The table at `path` of keys (any case), through tables and `GROUP`s.
    pub fn path(&self, path: &[&str]) -> Option<&Table> {
        let mut t = self;
        for key in path {
            t = t.get_ci(key)?.table()?;
        }
        Some(t)
    }
}

impl Value {
    /// The table this is, or a call's table (`GROUP { ... }`).
    pub fn table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            Value::Call(_, args) => args.first().and_then(Value::table),
            _ => None,
        }
    }

    /// The number this is (or `FLOAT(n)`'s); a blueprint's `''` for a number it lacks is none.
    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Call(_, args) if args.len() == 1 => args[0].num(),
            _ => None,
        }
    }

    /// The string this is (or `STRING('s')`'s).
    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Call(_, args) if args.len() == 1 => args[0].str(),
            _ => None,
        }
    }

    /// The boolean this is (or `BOOLEAN(b)`'s).
    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Call(_, args) if args.len() == 1 => args[0].bool(),
            _ => None,
        }
    }

    /// Three numbers: `VECTOR3(x, y, z)`, or a table `{x, y, z}`.
    pub fn vec3(&self) -> Option<[f64; 3]> {
        let items: &[Value] = match self {
            Value::Call(_, args) => args,
            Value::Table(t) => &t.array,
            _ => return None,
        };
        if items.len() < 3 {
            return None;
        }
        Some([items[0].num()?, items[1].num()?, items[2].num()?])
    }

    /// The numbers of a call or a table's bare values (`RECTANGLE(x0, y0, x1, y1)`).
    pub fn numbers(&self) -> Vec<f64> {
        let items: &[Value] = match self {
            Value::Call(_, args) => args,
            Value::Table(t) => &t.array,
            _ => &[],
        };
        items.iter().filter_map(Value::num).collect()
    }
}

/// A file's top-level statements in order: `name = value` (dotted names kept whole), and calls
/// as `(name, the call)`.
pub fn parse(text: &str) -> Result<Vec<(String, Value)>> {
    let mut p = Parser { s: text.as_bytes(), i: 0, depth: 0 };
    // (A byte-order mark, as the string tables have.)
    if p.s.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.i = 3;
    }
    let mut out = Vec::new();
    loop {
        p.space();
        if p.i >= p.s.len() {
            break;
        }
        if p.eat(b';') {
            continue;
        }
        let mut name = p.name().ok_or_else(|| p.error("a name"))?;
        // (`local x = ...`: a script's own name, read as any other.)
        if name == "local" {
            name = p.name().ok_or_else(|| p.error("a name"))?;
        }
        while p.peek() == Some(b'.') {
            p.i += 1;
            name.push('.');
            name.push_str(&p.name().ok_or_else(|| p.error("a name"))?);
        }
        p.space();
        if p.eat(b'=') {
            let value = p.value()?;
            out.push((name, value));
        } else {
            let call = p.call_args(name.clone())?;
            out.push((name, call));
        }
    }
    Ok(out)
}

/// The value of the first top-level assignment or call named `name` (any case).
pub fn top<'a>(statements: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    statements.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, wanted: &str) -> Error {
        let line = self.s[..self.i.min(self.s.len())].iter().filter(|&&c| c == b'\n').count() + 1;
        Error::Format(format!("Lua data, line {line}: expected {wanted}"))
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Skips white space and comments.
    fn space(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.i += 1;
            }
            let rest = &self.s[self.i..];
            if rest.starts_with(b"--") {
                self.i += 2;
                match long_bracket(&self.s[self.i..]) {
                    // `--[[ ... ]]`, `--[==[ ... ]==]`
                    Some(level) => self.i = skip_long(self.s, self.i, level),
                    None => self.line(),
                }
            } else if rest.starts_with(b"#") {
                self.line();
            } else {
                return;
            }
        }
    }

    /// Skips to the end of the line.
    fn line(&mut self) {
        while self.peek().is_some_and(|c| c != b'\n') {
            self.i += 1;
        }
    }

    /// Enters a value within others (a table, a call's arguments): an error past [`MAX_DEPTH`]
    /// (a file nesting them without end would run the reader out of stack).
    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("values less deeply nested"));
        }
        Ok(())
    }

    fn name(&mut self) -> Option<String> {
        self.space();
        let start = self.i;
        if !self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            return None;
        }
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.i += 1;
        }
        Some(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn value(&mut self) -> Result<Value> {
        self.space();
        match self.peek() {
            None => Err(self.error("a value")),
            Some(b'{') => Ok(Value::Table(self.table()?)),
            Some(b'\'' | b'"') => Ok(Value::Str(self.string()?)),
            Some(b'[') if long_bracket(&self.s[self.i..]).is_some() => Ok(Value::Str(self.long_string())),
            Some(c) if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() => Ok(Value::Num(self.number()?)),
            Some(_) => {
                let mut name = self.name().ok_or_else(|| self.error("a value"))?;
                // (A dotted name, a function another file defines: `Funcs.SomeCheck`.)
                while self.peek() == Some(b'.') && self.s.get(self.i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
                    self.i += 1;
                    name.push('.');
                    name.push_str(&self.name().ok_or_else(|| self.error("a name"))?);
                }
                match name.as_str() {
                    "nil" => Ok(Value::Nil),
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    _ => {
                        self.space();
                        if matches!(self.peek(), Some(b'(' | b'{' | b'\'' | b'"')) {
                            self.call_args(name)
                        } else {
                            // (A bare name: a constant the game would know. Kept as its name.)
                            Ok(Value::Str(name))
                        }
                    }
                }
            }
        }
    }

    /// The arguments of a call to `name`: `( v, v )`, `{ table }` or `'string'`.
    fn call_args(&mut self, name: String) -> Result<Value> {
        self.enter()?;
        self.space();
        let args = match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let mut args = Vec::new();
                loop {
                    self.space();
                    if self.eat(b')') {
                        break;
                    }
                    args.push(self.value()?);
                    self.space();
                    if !self.eat(b',') {
                        if !self.eat(b')') {
                            return Err(self.error("')'"));
                        }
                        break;
                    }
                }
                args
            }
            Some(b'{') => vec![Value::Table(self.table()?)],
            Some(b'\'' | b'"') => vec![Value::Str(self.string()?)],
            _ => return Err(self.error("'=' or a call")),
        };
        self.depth -= 1;
        Ok(Value::Call(name, args))
    }

    fn table(&mut self) -> Result<Table> {
        self.enter()?;
        self.space();
        self.i += 1; // '{'
        let mut t = Table::default();
        loop {
            self.space();
            if self.eat(b'}') {
                break;
            }
            // `[key] = v`, `name = v` or a bare value.
            let start = self.i;
            if self.peek() == Some(b'[') && long_bracket(&self.s[self.i..]).is_none() {
                self.i += 1;
                let key = self.value()?;
                if !self.eat(b']') || !self.eat(b'=') {
                    return Err(self.error("'] ='"));
                }
                let key = match key {
                    Value::Str(s) => s,
                    Value::Num(n) => format_number(n),
                    other => format!("{other:?}"),
                };
                let value = self.value()?;
                t.fields.push((key, value));
            } else if let Some(name) = self.name() {
                self.space();
                if self.peek() == Some(b'=') && self.s.get(self.i + 1) != Some(&b'=') {
                    self.i += 1;
                    let value = self.value()?;
                    t.fields.push((name, value));
                } else {
                    // A bare value that starts with a name (`true`, `GROUP {...}`).
                    self.i = start;
                    t.array.push(self.value()?);
                }
            } else {
                t.array.push(self.value()?);
            }
            self.space();
            if !self.eat(b',') && !self.eat(b';') {
                if !self.eat(b'}') {
                    return Err(self.error("',' or '}'"));
                }
                break;
            }
        }
        self.depth -= 1;
        Ok(t)
    }

    fn string(&mut self) -> Result<String> {
        let quote = self.s[self.i];
        self.i += 1;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None | Some(b'\n') => return Err(self.error("the string's end")),
                Some(c) if c == quote => {
                    self.i += 1;
                    break;
                }
                Some(b'\\') => {
                    self.i += 1;
                    let c = self.peek().ok_or_else(|| self.error("an escape"))?;
                    self.i += 1;
                    out.push(match c {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        other => other,
                    });
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// A long string (`[[ ... ]]`, `[==[ ... ]==]`), the parser at its opening bracket.
    fn long_string(&mut self) -> String {
        let level = long_bracket(&self.s[self.i..]).unwrap_or(0);
        let body = (self.i + level + 2).min(self.s.len());
        let close = closing(self.s, body, level);
        let text = String::from_utf8_lossy(&self.s[body..close.unwrap_or(self.s.len())]).into_owned();
        self.i = close.map_or(self.s.len(), |c| c + level + 2);
        text
    }

    fn number(&mut self) -> Result<f64> {
        let start = self.i;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.i += 1;
            self.space();
        }
        let digits = self.i;
        if self.s[self.i..].starts_with(b"0x") || self.s[self.i..].starts_with(b"0X") {
            self.i += 2;
            let hex = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let v = u64::from_str_radix(std::str::from_utf8(&self.s[hex..self.i]).unwrap_or("0"), 16).map_err(|_| self.error("a number"))? as f64;
            return Ok(if self.s[start] == b'-' { -v } else { v });
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
            self.i += 1;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.i += 1;
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let text = std::str::from_utf8(&self.s[digits..self.i]).unwrap_or("");
        let v: f64 = text.parse().map_err(|_| self.error("a number"))?;
        Ok(if self.s[start] == b'-' { -v } else { v })
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The level of the long bracket (`[`, `level` times `=`, `[`) that `s` starts with, if it does.
fn long_bracket(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'[') {
        return None;
    }
    let level = s[1..].iter().take_while(|&&c| c == b'=').count();
    (s.get(1 + level) == Some(&b'[')).then_some(level)
}

/// Where the long bracket of `level` whose body starts at `from` closes: its `]`, `level` times
/// `=`, `]` (none when it never does).
fn closing(s: &[u8], from: usize, level: usize) -> Option<usize> {
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', level));
    close.push(b']');
    find(&s[from.min(s.len())..], &close).map(|at| from + at)
}

/// Past the long comment or string of `level` opening at `at`: after its closing bracket, or
/// the text's end.
fn skip_long(s: &[u8], at: usize, level: usize) -> usize {
    closing(s, at + level + 2, level).map_or(s.len(), |c| c + level + 2)
}

/// A script as far as finding calls in it needs: its names, its strings, and its other
/// characters one by one; white space and comments left out.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Name(&'a [u8]),
    Str(String),
    Other(u8),
}

fn tokens(s: &[u8]) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    let line_end = |mut i: usize| {
        while i < s.len() && s[i] != b'\n' {
            i += 1;
        }
        i
    };
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"--") {
            i += 2;
            i = match long_bracket(&s[i..]) {
                Some(level) => skip_long(s, i, level),
                None => line_end(i),
            };
        } else if c == b'#' {
            i = line_end(i);
        } else if let Some(level) = long_bracket(&s[i..]) {
            let body = (i + level + 2).min(s.len());
            let close = closing(s, body, level);
            out.push(Token::Str(String::from_utf8_lossy(&s[body..close.unwrap_or(s.len())]).into_owned()));
            i = close.map_or(s.len(), |c| c + level + 2);
        } else if c == b'\'' || c == b'"' {
            let mut text = Vec::new();
            let mut j = i + 1;
            while j < s.len() && s[j] != c && s[j] != b'\n' {
                if s[j] == b'\\' && j + 1 < s.len() {
                    text.push(s[j + 1]);
                    j += 2;
                } else {
                    text.push(s[j]);
                    j += 1;
                }
            }
            out.push(Token::Str(String::from_utf8_lossy(&text).into_owned()));
            i = (j + 1).min(s.len());
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
                i += 1;
            }
            out.push(Token::Name(&s[start..i]));
        } else {
            out.push(Token::Other(c));
            i += 1;
        }
    }
    out
}

/// The first string among the arguments of the first call to `name` in a script that has one
/// (`ScenarioFramework.SetPlayableArea('AREA_1', false)` gives `AREA_1`), what is commented out
/// left out (`--`, `--[[ ]]`, `--[==[ ]==]`, `#`), and never a string that only mentions it.
pub fn call_string(script: &str, name: &str) -> Option<String> {
    let tokens = tokens(script.as_bytes());
    for (k, t) in tokens.iter().enumerate() {
        if *t != Token::Name(name.as_bytes()) || tokens.get(k + 1) != Some(&Token::Other(b'(')) {
            continue;
        }
        let mut depth = 0;
        for t in &tokens[k + 1..] {
            match t {
                Token::Other(b'(') => depth += 1,
                Token::Other(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Token::Str(s) => return Some(s.clone()),
                _ => {}
            }
        }
    }
    None
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_and_a_save_file_read_as_the_game_writes_them() {
        let scenario = "version = 3\r\nScenarioInfo = {\r\n    name = \"Drake's Ravine\",\r\n    description = '<LOC SCMP_003_Description>Text.',\r\n    size = {1024, 1024},\r\n    norushradius = 90.000000,\r\n    Configurations = {\r\n        ['standard'] = {\r\n            teams = {\r\n                { name = 'FFA', armies = {'ARMY_1','ARMY_2',} },\r\n            },\r\n            customprops = {\r\n                ['ExtraArmies'] = STRING( 'ARMY_9 NEUTRAL_CIVILIAN' ),\r\n            },\r\n        },\r\n    }}\r\n";
        let statements = parse(scenario).unwrap();
        assert_eq!(top(&statements, "version").and_then(Value::num), Some(3.0));
        let info = top(&statements, "ScenarioInfo").and_then(Value::table).unwrap();
        assert_eq!(info.get("name").and_then(Value::str), Some("Drake's Ravine"));
        assert_eq!(info.get("size").unwrap().numbers(), [1024.0, 1024.0]);
        let standard = info.path(&["Configurations", "standard"]).unwrap();
        let armies = &standard.get("teams").and_then(Value::table).unwrap().array[0].table().unwrap().get("armies").and_then(Value::table).unwrap().array;
        assert_eq!(armies.iter().filter_map(Value::str).collect::<Vec<_>>(), ["ARMY_1", "ARMY_2"]);
        assert_eq!(standard.path(&["customprops"]).unwrap().get("ExtraArmies").and_then(Value::str), Some("ARMY_9 NEUTRAL_CIVILIAN"));

        let save = "--[[                                                                           ]]--\r\n--[[  Automatically generated code (do not edit)                               ]]--\r\nScenario = {\r\n  next_area_id = '1',\r\n  Props = {\r\n  },\r\n  MasterChain = {\r\n    ['_MASTERCHAIN_'] = {\r\n      Markers = {\r\n        ['Mass 01'] = {\r\n          ['size'] = FLOAT( 1.000000 ),\r\n          ['resource'] = BOOLEAN( true ),\r\n          ['type'] = STRING( 'Mass' ),\r\n          ['position'] = VECTOR3( 81.5, 6.75917e-005, -12.5 ),\r\n        },\r\n      },\r\n    },\r\n  },\r\n  Armies = \r\n  {\r\n    ['ARMY_9'] = {\r\n      Units = GROUP {\r\n        orders = '',\r\n        Units = {\r\n          ['UNIT_1'] = { type = 'urb5101', Position = { 109.5, 4.91, 101.5 }, Orientation = { 0, 0.0087, 0 } },\r\n        },\r\n      },\r\n    },\r\n  },\r\n}\r\n";
        let statements = parse(save).unwrap();
        let scenario = top(&statements, "Scenario").and_then(Value::table).unwrap();
        let markers = scenario.path(&["MasterChain", "_MASTERCHAIN_", "Markers"]).unwrap();
        let mass = markers.get("Mass 01").and_then(Value::table).unwrap();
        assert_eq!(mass.get("type").and_then(Value::str), Some("Mass"));
        assert_eq!(mass.get("resource").and_then(Value::bool), Some(true));
        assert_eq!(mass.get("size").and_then(Value::num), Some(1.0));
        let p = mass.get("position").and_then(Value::vec3).unwrap();
        assert_eq!((p[0], p[2]), (81.5, -12.5));
        assert!((p[1] - 6.75917e-5).abs() < 1e-12);
        let unit = scenario.path(&["Armies", "ARMY_9", "Units", "Units", "UNIT_1"]).unwrap();
        assert_eq!(unit.get("Position").and_then(Value::vec3), Some([109.5, 4.91, 101.5]));
    }

    #[test]
    fn a_prop_blueprint_reads_with_its_comments_and_placeholders() {
        let bp = "# A tree\nPropBlueprint {\n    Categories = {\n        'RECLAIMABLE',\n    },\n    Economy = {\n        ReclaimEnergyMax = '',\n        ReclaimMassMax = 1,\n    },\n    Physics = { BlockPath = false },\n    ScriptClass = 'Tree',\n    Sound = Sound { Bank = 'X', Cue = 'Y' },\n    SizeX = .1,\n}\n";
        let statements = parse(bp).unwrap();
        let bp = top(&statements, "PropBlueprint").and_then(Value::table).unwrap();
        assert_eq!(bp.path(&["Economy"]).unwrap().get("ReclaimEnergyMax").and_then(Value::num), None);
        assert_eq!(bp.path(&["economy"]).unwrap().get("ReclaimMassMax").and_then(Value::num), Some(1.0));
        assert_eq!(bp.path(&["Physics"]).unwrap().get("BlockPath").and_then(Value::bool), Some(false));
        assert_eq!(bp.get("ScriptClass").and_then(Value::str), Some("Tree"));
        assert_eq!(bp.get("SizeX").and_then(Value::num), Some(0.1));
        assert!(parse("Scenario = { ['x'] = ").is_err());
        assert!(parse(&format!("A = {}", "{".repeat(100))).is_err());
    }

    /// Long comments and strings of any level, and calls nested past all reason refused (not
    /// read until the stack runs out: a player's map from the vault is anybody's file).
    #[test]
    fn long_brackets_of_every_level_and_deep_nesting() {
        let text = "--[==[ a comment ]] still the comment ]==]\nA = { x = [=[a ]] string]=], y = 2, --[[ c ]] z = [[s]] }\n";
        let statements = parse(text).unwrap();
        let a = top(&statements, "A").and_then(Value::table).unwrap();
        assert_eq!(a.get("x").and_then(Value::str), Some("a ]] string"));
        assert_eq!(a.get("y").and_then(Value::num), Some(2.0));
        assert_eq!(a.get("z").and_then(Value::str), Some("s"));
        let deep = format!("A = {}1{}", "F(".repeat(10_000), ")".repeat(10_000));
        assert!(parse(&deep).is_err());
        let fine = format!("A = {}1{}", "F(".repeat(20), ")".repeat(20));
        assert!(parse(&fine).is_ok());
    }

    /// A call's first string found in what a script runs: never in a comment of any kind, nor
    /// in a string that only names the function.
    #[test]
    fn a_scripts_calls_are_found_and_its_comments_skipped() {
        let area = |s: &str| call_string(s, "SetPlayableArea");
        assert_eq!(area("ScenarioFramework.SetPlayableArea('AREA_1', false)").as_deref(), Some("AREA_1"));
        assert_eq!(area("--ScenarioFramework.SetPlayableArea('AREA_1', false)\nfoo('x')"), None);
        assert_eq!(area("# ScenarioFramework.SetPlayableArea('AREA_1')"), None);
        assert_eq!(area("--[[\nScenarioFramework.SetPlayableArea('AREA_1')\n]]"), None);
        assert_eq!(area("--[==[\nSetPlayableArea('AREA_1') ]] still a comment\n]==]\nSetPlayableArea(\"AREA_2\")").as_deref(), Some("AREA_2"));
        assert_eq!(area("LOG('SetPlayableArea(\\'AREA_1\\')')"), None);
        assert_eq!(area("SetPlayableArea(ScenarioUtils.AreaToRect('AREA_3'), false)").as_deref(), Some("AREA_3"));
        // A call without a string, then one with.
        assert_eq!(area("function SetPlayableArea(rect)\nend\nSetPlayableArea('AREA_4')").as_deref(), Some("AREA_4"));
        assert_eq!(area("SetPlayableAreaLater('AREA_5')"), None);
    }
}
