//! What Forged Alliance's scripts say about its units and projectiles that their blueprints do
//! not: a unit's weapons and intel its script switches off as it is made (a Seraphim commander's
//! missile, a sniper's second gun, the Cybran commander's stealth), and what a shell its script
//! splits into (the Salvation's fragments). The scripts are Lua programs; nothing is run. They are
//! read as text for these few statements, each in the function and the block it stands in.
//!
//! - **At birth** ([`Birth`]): a unit is made with every weapon on, then its weapons' `OnCreate`
//!   (`self:SetWeaponEnabled(false)` in a weapon's class), its own `OnCreate` and its
//!   `OnStopBeingBuilt` run, in that order, each statement in turn: `SetWeaponEnabledByLabel
//!   ('<label>', true|false)` and `EnableUnitIntel('<kind>')` / `DisableUnitIntel('<kind>')`.
//!   Statements inside an `if` there (a layer's: a Wagner's guns on land, its torpedo under the
//!   water) depend on where the unit is: what any of them, or the unit's `OnLayerChange`, turns on
//!   is kept ([`Birth::by_layer`]), for TA Reborn's own rules say where each fires from.
//! - **Fragments** ([`fragments`]): a projectile whose `OnImpact` makes child projectiles
//!   (`CreateChildProjectile(ChildProjectileBP)`, once and in a loop of `numProjectiles`), each
//!   with its parent's damage, or a share of it where the script divides it
//!   (`DamageAmount / numProjectiles`).

/// What a unit's script switches off as it is made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Birth {
    /// Weapons (labels as written) off once the unit is made.
    pub weapons_off: Vec<String>,
    /// Weapons switched on or off by where the unit is (in an `if` of its making, or by its
    /// `OnLayerChange`): never counted off.
    pub by_layer: Vec<String>,
    /// Intel (`Radar`, `Sonar`, `RadarStealth`, `SonarStealth`, `Cloak`, `Omni`...) off once made.
    pub intel_off: Vec<String>,
}

impl Birth {
    /// Whether the weapon `label` is off once the unit is made.
    pub fn weapon_off(&self, label: &str) -> bool {
        self.weapons_off.iter().any(|l| l.eq_ignore_ascii_case(label)) && !self.by_layer.iter().any(|l| l.eq_ignore_ascii_case(label))
    }

    /// Whether the intel `kind` is off once the unit is made.
    pub fn intel_off(&self, kind: &str) -> bool {
        self.intel_off.iter().any(|k| k.eq_ignore_ascii_case(kind))
    }
}

/// A script's words: names, strings and single marks, comments and strings' insides left out of
/// the marks (`#` and `--` comments to the line's end, `--[[ ]]` blocks).
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Name(String),
    Str(String),
    Num(String),
    Mark(char),
}

fn tokens(text: &str) -> Vec<Token> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'#' || (c == b'-' && b.get(i + 1) == Some(&b'-')) {
            if c == b'-' && b.get(i + 2) == Some(&b'[') && b.get(i + 3) == Some(&b'[') {
                i = text[i..].find("]]").map_or(b.len(), |e| i + e + 2);
            } else {
                i = text[i..].find('\n').map_or(b.len(), |e| i + e + 1);
            }
        } else if c == b'\'' || c == b'"' {
            let mut j = i + 1;
            let mut s = Vec::new();
            while j < b.len() && b[j] != c && b[j] != b'\n' {
                if b[j] == b'\\' && j + 1 < b.len() {
                    j += 1;
                }
                s.push(b[j]);
                j += 1;
            }
            out.push(Token::Str(String::from_utf8_lossy(&s).into_owned()));
            i = j + 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let j = (i..b.len()).find(|&j| !(b[j].is_ascii_alphanumeric() || b[j] == b'_')).unwrap_or(b.len());
            out.push(Token::Name(text[i..j].to_string()));
            i = j;
        } else if c.is_ascii_digit() {
            let j = (i..b.len()).find(|&j| !(b[j].is_ascii_alphanumeric() || b[j] == b'.')).unwrap_or(b.len());
            out.push(Token::Num(text[i..j].to_string()));
            i = j;
        } else {
            out.push(Token::Mark(c as char));
            i += 1;
        }
    }
    out
}

fn is_name(t: &Token, name: &str) -> bool {
    matches!(t, Token::Name(n) if n == name)
}

/// The end of the block that the opener at `start` opens (`function`, `if`, `do`, `repeat`: the
/// index just past its `end` or `until`).
fn block_end(t: &[Token], start: usize) -> usize {
    let mut depth = 0i32;
    for (k, tok) in t.iter().enumerate().skip(start) {
        if let Token::Name(n) = tok {
            match n.as_str() {
                "function" | "if" | "do" | "repeat" => depth += 1,
                "end" | "until" => {
                    depth -= 1;
                    if depth == 0 {
                        return k + 1;
                    }
                }
                _ => {}
            }
        }
    }
    t.len()
}

/// The bodies of the functions assigned to `name` (`name = function(...) ... end`), as token
/// ranges (the body after the parameters, its `end` left out).
fn functions(t: &[Token], name: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = from;
    while k + 2 < to {
        if is_name(&t[k], name) && t[k + 1] == Token::Mark('=') && is_name(&t[k + 2], "function") {
            let end = block_end(t, k + 2).min(to);
            // (The body begins after the parameters' closing bracket.)
            let body = (k + 3..end).find(|&j| t[j] == Token::Mark(')')).map_or(k + 3, |j| j + 1);
            out.push((body, end.saturating_sub(1)));
            k = end;
        } else {
            k += 1;
        }
    }
    out
}

/// The calls `method('<first>', <second>)` in `t[from..to]`, in order: the first argument (a
/// string) and the second (`true`, `false` or none), and whether the call stands inside an `if`
/// or loop of that range.
fn calls(t: &[Token], method: &str, from: usize, to: usize) -> Vec<(String, Option<bool>, bool)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut k = from;
    while k < to {
        if let Token::Name(n) = &t[k] {
            match n.as_str() {
                "if" | "do" | "repeat" | "function" => depth += 1,
                "end" | "until" => depth -= 1,
                _ if n == method && t.get(k + 1) == Some(&Token::Mark('(')) => {
                    if let Some(Token::Str(first)) = t.get(k + 2) {
                        let second = match (t.get(k + 3), t.get(k + 4)) {
                            (Some(Token::Mark(',')), Some(Token::Name(v))) if v == "true" => Some(true),
                            (Some(Token::Mark(',')), Some(Token::Name(v))) if v == "false" => Some(false),
                            _ => None,
                        };
                        out.push((first.clone(), second, depth > 0));
                    }
                }
                _ => {}
            }
        }
        k += 1;
    }
    out
}

/// What the unit script `text` switches off as the unit is made.
pub fn birth(text: &str) -> Birth {
    let t = tokens(text);
    let mut off: Vec<String> = Vec::new();
    let mut by_layer: Vec<String> = Vec::new();
    let mut intel_off: Vec<String> = Vec::new();
    let set = |list: &mut Vec<String>, name: &str, on: bool| {
        list.retain(|l| !l.eq_ignore_ascii_case(name));
        if !on {
            list.push(name.to_string());
        }
    };
    // The weapons' classes (`Weapons = { <Label> = Class(...) { ... }, ... }`): those whose own
    // OnCreate switches them off.
    if let Some(at) = (0..t.len().saturating_sub(2)).find(|&k| is_name(&t[k], "Weapons") && t[k + 1] == Token::Mark('=') && t[k + 2] == Token::Mark('{')) {
        let close = table_end(&t, at + 2);
        let mut k = at + 3;
        while k + 2 < close {
            if let Token::Name(label) = &t[k]
                && t[k + 1] == Token::Mark('=')
                && is_name(&t[k + 2], "Class")
            {
                // Its class body: the next `{` and its match.
                let Some(open) = (k + 3..close).find(|&j| t[j] == Token::Mark('{')) else { break };
                let end = table_end(&t, open);
                for (from, to) in functions(&t, "OnCreate", open, end) {
                    let disables = (from..to).any(|j| {
                        is_name(&t[j], "SetWeaponEnabled") && t.get(j + 1) == Some(&Token::Mark('(')) && t.get(j + 2).is_some_and(|v| is_name(v, "false"))
                    });
                    if disables {
                        set(&mut off, label, false);
                    }
                }
                k = end;
            } else {
                k += 1;
            }
        }
    }
    // The unit's own OnCreate, then OnStopBeingBuilt (the functions outside the weapons' table:
    // the last of each name, a script's own overriding).
    let weapons_table = (0..t.len().saturating_sub(2)).find(|&k| is_name(&t[k], "Weapons") && t[k + 1] == Token::Mark('=') && t[k + 2] == Token::Mark('{')).map(|at| (at, table_end(&t, at + 2)));
    let outside = |range: &(usize, usize)| weapons_table.is_none_or(|(a, b)| range.0 > b || range.1 < a);
    for name in ["OnCreate", "OnStopBeingBuilt"] {
        let Some(body) = functions(&t, name, 0, t.len()).into_iter().rfind(outside) else { continue };
        for (label, on, inside) in calls(&t, "SetWeaponEnabledByLabel", body.0, body.1) {
            match (on, inside) {
                (Some(on), false) => set(&mut off, &label, on),
                (Some(true), true) if !by_layer.iter().any(|l| l.eq_ignore_ascii_case(&label)) => by_layer.push(label),
                _ => {}
            }
        }
        for (method, on) in [("EnableUnitIntel", true), ("DisableUnitIntel", false)] {
            for (kind, _, inside) in calls(&t, method, body.0, body.1) {
                if !inside {
                    set(&mut intel_off, &kind, on);
                }
            }
        }
    }
    for body in functions(&t, "OnLayerChange", 0, t.len()) {
        for (label, on, _) in calls(&t, "SetWeaponEnabledByLabel", body.0, body.1) {
            if on == Some(true) && !by_layer.iter().any(|l| l.eq_ignore_ascii_case(&label)) {
                by_layer.push(label);
            }
        }
    }
    Birth { weapons_off: off, by_layer, intel_off }
}

/// The index just past the `}` closing the table opened at `open` (`t[open]` is `{`).
fn table_end(t: &[Token], open: usize) -> usize {
    let mut depth = 0i32;
    for (k, tok) in t.iter().enumerate().skip(open) {
        match tok {
            Token::Mark('{') => depth += 1,
            Token::Mark('}') => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    t.len()
}

/// What a projectile splits into as it lands: the child's blueprint (as written), how many of
/// them, and the share of the parent's damage each carries (1: all of it).
#[derive(Clone, Debug, PartialEq)]
pub struct Fragments {
    pub child: String,
    pub count: u32,
    pub share: f64,
}

/// What the projectile script `text` splits its projectile into, if it does.
pub fn fragments(text: &str) -> Option<Fragments> {
    let t = tokens(text);
    let impact = functions(&t, "OnImpact", 0, t.len()).into_iter().next()?;
    let (from, to) = impact;
    // The child's blueprint: `ChildProjectileBP = '<path>'`.
    let child = (from..to.saturating_sub(2)).find_map(|k| match (&t[k], &t[k + 1], &t[k + 2]) {
        (Token::Name(n), Token::Mark('='), Token::Str(path)) if n == "ChildProjectileBP" => Some(path.clone()),
        _ => None,
    })?;
    // `numProjectiles = N`.
    let many = (from..to.saturating_sub(2)).find_map(|k| match (&t[k], &t[k + 1], &t[k + 2]) {
        (Token::Name(n), Token::Mark('='), Token::Num(v)) if n == "numProjectiles" => v.parse::<u32>().ok(),
        _ => None,
    });
    // Each `CreateChildProjectile(` makes one, a loop's `numProjectiles` of them.
    let mut count = 0u32;
    let mut k = from;
    let mut loops: Vec<usize> = Vec::new();
    while k < to {
        match &t[k] {
            Token::Name(n) if n == "for" || n == "while" => {
                let end = block_end(&t, (k..to).find(|&j| is_name(&t[j], "do")).unwrap_or(k));
                loops.push(end);
            }
            Token::Name(n) if n == "CreateChildProjectile" && t.get(k + 1) == Some(&Token::Mark('(')) => {
                loops.retain(|&end| end > k);
                count += if loops.is_empty() { 1 } else { many.unwrap_or(1) };
            }
            _ => {}
        }
        k += 1;
    }
    if count <= 1 {
        return None;
    }
    // A script dividing its damage among them (`DamageAmount / numProjectiles`).
    let divided = (from..to.saturating_sub(2)).any(|k| is_name(&t[k], "DamageAmount") && t[k + 1] == Token::Mark('/') && is_name(&t[k + 2], "numProjectiles"));
    let share = match (divided, many) {
        (true, Some(n)) if n > 0 => 1.0 / n as f64,
        _ => 1.0,
    };
    Some(Fragments { child, count, share })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit made with a missile its class switches off, a toggled gun its OnCreate switches off,
    /// a gun its OnStopBeingBuilt switches back on, weapons switched by the layer it stands on,
    /// and stealth and sonar switched off once built (the Cybran commander's).
    const UNIT: &str = r#"
local Base = import('/lua/x.lua').Base
Thing = Class(Base) {
    Weapons = {
        MainGun = Class(Gun) {},
        Missile = Class(Launcher) {
            OnCreate = function(self)
                Launcher.OnCreate(self)
                self:SetWeaponEnabled(false)
            end,
        },
        OverCharge = Class(Cannon) {
            OnEnableWeapon = function(self)
                self.unit:SetWeaponEnabledByLabel('MainGun', false)
            end,
        },
        Bolter = Class(Gun) {},
        Torpedo = Class(Tube) {},
    },
    OnCreate = function(self)
        Base.OnCreate(self)
        self:SetWeaponEnabledByLabel('SniperGun', false)  # toggled on later
        self:SetWeaponEnabledByLabel('Rifle', false)
    end,
    OnStopBeingBuilt = function(self, builder, layer)
        Base.OnStopBeingBuilt(self, builder, layer)
        self:SetWeaponEnabledByLabel('Rifle', true)
        -- self:SetWeaponEnabledByLabel('MainGun', false)
        if layer == 'Land' then
            self:SetWeaponEnabledByLabel('Bolter', true)
            self:SetWeaponEnabledByLabel('Torpedo', false)
        elseif layer == 'Seabed' then
            self:SetWeaponEnabledByLabel('Bolter', false)
            self:SetWeaponEnabledByLabel('Torpedo', true)
        end
        self:DisableUnitIntel('RadarStealth')
        self:DisableUnitIntel('Sonar')
    end,
    OnLayerChange = function(self, new, old)
        if new == 'Seabed' then self:SetWeaponEnabledByLabel('Torpedo', true) end
    end,
}
"#;

    #[test]
    fn what_a_unit_switches_off_as_it_is_made() {
        let b = birth(UNIT);
        assert!(b.weapon_off("Missile"), "its class's OnCreate: {b:?}");
        assert!(b.weapon_off("snipergun"), "its OnCreate: {b:?}");
        assert!(!b.weapon_off("Rifle"), "switched back on once built: {b:?}");
        assert!(!b.weapon_off("MainGun"), "off only while the overcharge is: {b:?}");
        assert!(!b.weapon_off("Bolter") && !b.weapon_off("Torpedo"), "by the layer: {b:?}");
        assert!(b.intel_off("radarstealth") && b.intel_off("Sonar") && !b.intel_off("Radar"), "{b:?}");
        assert_eq!(birth("Unit = Class(Base) { OnCreate = function(self) end, }"), Birth::default());
    }

    #[test]
    fn what_a_shell_splits_into() {
        // One along its path and five spread round (each with all of its parent's damage), as
        // the Salvation's shells do; a script dividing the damage among eight.
        let shell = r#"
Shell = Class(Base) {
    OnImpact = function(self, TargetType, TargetEntity)
        local ChildProjectileBP = '/projectiles/Frag02/Frag02_proj.bp'
        self:CreateChildProjectile(ChildProjectileBP):SetVelocity(vx, vy, vz):PassDamageData(self.DamageData)
        local numProjectiles = 5
        for i = 0, (numProjectiles -1) do
            local proj = self:CreateChildProjectile(ChildProjectileBP)
            proj:PassDamageData(self.DamageData)
        end
        self:Destroy()
    end,
}
"#;
        assert_eq!(fragments(shell), Some(Fragments { child: "/projectiles/Frag02/Frag02_proj.bp".into(), count: 6, share: 1.0 }));
        let divided = r#"
Missile = Class(Base) {
    OnImpact = function(self, TargetType, TargetEntity)
        local numProjectiles = 8
        local ChildProjectileBP = '/projectiles/M02/M02_proj.bp'
        self.DamageData.DamageAmount = self.DamageData.DamageAmount / numProjectiles
        for i = 0, (numProjectiles -1) do
            local proj = self:CreateChildProjectile(ChildProjectileBP)
        end
    end,
}
"#;
        assert_eq!(fragments(divided), Some(Fragments { child: "/projectiles/M02/M02_proj.bp".into(), count: 8, share: 0.125 }));
        // A commented-out child and none at all split nothing.
        assert_eq!(fragments("P = Class(B) { OnImpact = function(self) #self:CreateChildProjectile(X)\n end, }"), None);
        assert_eq!(fragments("P = Class(B) { }"), None);
    }
}
