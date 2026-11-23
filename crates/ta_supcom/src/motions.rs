//! What Forged Alliance's unit scripts move as their units work (docs/SUPCOM_UNITS.md): the parts
//! they spin (`CreateRotator`) and slide (`CreateSlider`), and the blueprint's animations they play
//! (`PlayAnim`), each with when it moves: from the unit's making, while it works (once it is built,
//! and while it is switched on), or while it builds. The scripts are read as text, as
//! [`crate::behaviour`] reads their other statements: nothing is run.
//!
//! A script's functions are its unit's handlers, which the engine calls as the unit is made
//! (`OnCreate`), finished (`OnStopBeingBuilt`), switched on and off (`OnIntelEnabled` and
//! `OnIntelDisabled`, `OnProductionUnpaused` and `OnProductionPaused`, `OnShieldEnabled`...) and as
//! it builds (`OnStartBuild`, a factory's `StartArmsMoving`); a state's `Main` runs as the script
//! changes into it (`ChangeState(self, self.OpenState)`), any other function as a handler calls it
//! or forks it. Each runs when what leads to it does, and what it sets is read so:
//!
//! - a part's speed or place set where the unit starts working (or building) is its working one;
//!   set where it stops (switched off, paused, damaged, killed, upgrading) it is how it stops, not
//!   a speed of its work. One set only as the unit is made moves from then on;
//! - speeds set both ways in a loop make the part swing to and fro, a stop between (the Cybran
//!   radars' dishes); places set in a loop, it shuttles between them (a factory's arms);
//! - a number the script works out is taken where it can be: `Random(a, b)` as its middle, a value
//!   the script keeps (`self.Goal`) as it is first set, a factor it does not know (a random way
//!   round) as 1;
//! - the weapons' classes are left out (a defence's dome turning as it fires is its weapon's).
//!
//! Manipulators are known by where the script keeps them: `self.Rotator`, a table of them
//! (`self.Spinners = { Spinner1 = CreateRotator(...) }`, gone through with `for k, v in
//! self.Spinners do`), a local, or none (`self.Trash:Add(CreateRotator(...))`: what its own call
//! says alone); calls chained on one (`CreateRotator(...):SetTargetSpeed(0)`) are its own.

use std::collections::BTreeMap;

use crate::behaviour::{Token, block_end, is_name, table_end, tokens};

/// When a part moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum When {
    /// From the unit's making (`OnCreate`), even while it is built.
    Made,
    /// While it works: once it is built, while it is switched on.
    Working,
    /// While it builds (a factory's arms).
    Building,
}

/// A part spinning about one of its bone's own axes.
#[derive(Clone, Debug, PartialEq)]
pub struct Spin {
    pub bone: String,
    /// The bone's axis it turns about: 0 x, 1 y, 2 z.
    pub axis: usize,
    /// Degrees a second, signed as Forged Alliance turns (about the axis by the right hand); a
    /// swing's first way.
    pub speed: f64,
    /// Degrees a second gained (or lost) each second (0: at once).
    pub accel: f64,
    pub when: When,
    /// It swings to and fro rather than round (its speeds set both ways in a loop): up to `speed`
    /// one way and back to a stop, then the other way; the longest pause it makes at each end
    /// (seconds, some share of it at random; 0 none).
    pub sway: Option<f64>,
}

/// A part sliding along its bone's own axes (mesh units).
#[derive(Clone, Debug, PartialEq)]
pub struct Slide {
    pub bone: String,
    /// Where it is put as the unit is made (none: where it rests).
    pub made: Option<[f64; 3]>,
    /// Where it goes as it moves: one place (and back to where it was made, or rests, as it
    /// stops), or the places it shuttles between in turn.
    pub goals: Vec<[f64; 3]>,
    pub shuttle: bool,
    /// Mesh units a second as it goes (0: at once), and as it goes back when it stops.
    pub speed: f64,
    pub back: f64,
    pub when: When,
}

/// An animation of its blueprint's that the script plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// The blueprint's `Display` key naming it (`AnimationOpen`, `LoopingAnimation`), or its path
    /// where the script names one (lower case).
    pub animation: String,
    /// Played round and round (`PlayAnim(anim, true)`, or played again in a loop), rather than once.
    pub looped: bool,
    /// How fast (`SetRate`: 1 as made).
    pub rate: f64,
    pub when: When,
}

/// What a unit's script moves as it works.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Motions {
    /// The class it is made of (`Class(TMassCollectionUnit)`), whose own ways the script keeps.
    pub base: String,
    pub spins: Vec<Spin>,
    pub slides: Vec<Slide>,
    pub plays: Vec<Play>,
}

/// Where a statement runs: as the unit is made, as it works, as it builds, or as it stops (in
/// order of what wins where a function runs in more than one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Context {
    Made,
    Working,
    Building,
    Stopping,
}

impl Context {
    fn when(self) -> Option<When> {
        match self {
            Context::Made => Some(When::Made),
            Context::Working => Some(When::Working),
            Context::Building => Some(When::Building),
            Context::Stopping => None,
        }
    }
}

/// When the engine calls a handler of this name.
fn handler(name: &str) -> Option<Context> {
    Some(match name {
        "OnCreate" => Context::Made,
        "OnStopBeingBuilt" | "PlayActiveAnimation" | "OnIntelEnabled" | "OnProductionUnpaused" | "OnShieldEnabled" | "OnActive" | "OnConsumptionActive" => Context::Working,
        "OnStartBuild" | "StartArmsMoving" | "MovingArmsThread" | "StartBuildFx" | "OnPrepareArmToBuild" => Context::Building,
        "OnIntelDisabled" | "OnProductionPaused" | "OnShieldDisabled" | "OnInActive" | "OnConsumptionInActive" | "StopArmsMoving" | "StopBuildFx" | "OnStopBuild"
        | "OnFailedToBuild" | "OnKilled" | "OnDestroy" | "OnDamage" | "OnStopBuilderTracking" | "OnPaused" => Context::Stopping,
        _ => return None,
    })
}

/// States the engine changes into as the unit stops: what is set there is how it stops.
fn stopping_state(name: &str) -> bool {
    matches!(name, "UpgradingState" | "DeadState" | "DeathState")
}

/// A named function of the script: its name, the state it is in (if any) and its body's tokens.
struct Fun {
    name: String,
    state: Option<usize>,
    from: usize,
    to: usize,
}

/// A manipulator the script makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Rotator,
    Slider,
    Animator,
}

struct Manip {
    kind: Kind,
    bone: Option<String>,
    axis: usize,
    /// Where it is kept (`self.Rotator`, `self.Spinners.Spinner1`, a local's name).
    path: Option<String>,
    /// Its own call's numbers after the bone (and axis).
    args: Vec<Option<f64>>,
    at: usize,
}

/// A method called on manipulators.
struct Call {
    manips: Vec<usize>,
    method: String,
    /// Its arguments' token ranges.
    args: Vec<(usize, usize)>,
    context: Option<Context>,
    /// The loop (`while`, `repeat`) it stands in, by its first token.
    in_loop: Option<usize>,
    function: Option<usize>,
    at: usize,
}

/// What the unit script `text` moves as its unit works.
pub fn motions(text: &str) -> Motions {
    let t = tokens(text);
    let weapons = (0..t.len().saturating_sub(2)).find(|&k| is_name(&t[k], "Weapons") && t[k + 1] == Token::Mark('=') && t[k + 2] == Token::Mark('{')).map(|at| (at, table_end(&t, at + 2)));
    let outside = |k: usize| weapons.is_none_or(|(a, b)| k < a || k >= b);
    let base = base_class(&t);
    // States, and functions (those of the weapons' classes left out).
    let mut states: Vec<(String, usize, usize)> = Vec::new();
    for k in 0..t.len().saturating_sub(3) {
        if let Token::Name(name) = &t[k]
            && t[k + 1] == Token::Mark('=')
            && is_name(&t[k + 2], "State")
            && outside(k)
        {
            let open = if t[k + 3] == Token::Mark('{') { Some(k + 3) } else { (k + 3..t.len()).find(|&j| t[j] == Token::Mark('{')) };
            if let Some(open) = open {
                states.push((name.clone(), open, table_end(&t, open)));
            }
        }
    }
    let mut funs: Vec<Fun> = Vec::new();
    for k in 0..t.len().saturating_sub(2) {
        if let Token::Name(name) = &t[k]
            && t[k + 1] == Token::Mark('=')
            && is_name(&t[k + 2], "function")
            && outside(k)
        {
            let end = block_end(&t, k + 2);
            let from = (k + 3..end).find(|&j| t[j] == Token::Mark(')')).map_or(k + 3, |j| j + 1);
            let state = states.iter().position(|&(_, a, b)| k > a && k < b);
            funs.push(Fun { name: name.clone(), state, from, to: end.saturating_sub(1) });
        }
    }
    // The innermost function each token stands in.
    let function_at = |k: usize| -> Option<usize> { funs.iter().enumerate().filter(|(_, f)| k >= f.from && k < f.to).min_by_key(|(_, f)| f.to - f.from).map(|(i, _)| i) };
    let contexts = contexts(&t, &funs, &states, &function_at);
    let context_at = |k: usize| function_at(k).and_then(|f| contexts[f]);
    // Loops: `while ... do ... end`, `repeat ... until`.
    let mut loops: Vec<(usize, usize)> = Vec::new();
    for k in 0..t.len() {
        if is_name(&t[k], "while") {
            if let Some(d) = (k + 1..t.len()).find(|&j| is_name(&t[j], "do")) {
                loops.push((k, block_end(&t, d)));
            }
        } else if is_name(&t[k], "repeat") {
            loops.push((k, block_end(&t, k)));
        }
    }
    let loop_at = |k: usize, function: Option<usize>| -> Option<usize> {
        let within = |a: usize| function.is_none_or(|f| a >= funs[f].from && a < funs[f].to);
        loops.iter().filter(|&&(a, b)| k > a && k < b && within(a)).max_by_key(|&&(a, _)| a).map(|&(a, _)| a)
    };
    // The values the script keeps (`self.Goal = ...`), by their first setting.
    let mut kept: BTreeMap<String, usize> = BTreeMap::new();
    for k in 0..t.len().saturating_sub(4) {
        if is_name(&t[k], "self") && t[k + 1] == Token::Mark('.') && t[k + 3] == Token::Mark('=') && t[k + 4] != Token::Mark('=') && (k == 0 || t[k - 1] != Token::Mark('.')) && outside(k)
            && let Token::Name(name) = &t[k + 2]
        {
            kept.entry(name.clone()).or_insert(k + 4);
        }
    }
    let number = |from: usize, to: usize| -> Option<f64> { Numbers { t: &t, kept: &kept }.range(from, to) };
    // The manipulators.
    let mut manips: Vec<Manip> = Vec::new();
    for k in 1..t.len().saturating_sub(1) {
        let kind = match &t[k] {
            Token::Name(n) if n == "CreateRotator" => Kind::Rotator,
            Token::Name(n) if n == "CreateSlider" => Kind::Slider,
            Token::Name(n) if n == "CreateAnimator" => Kind::Animator,
            _ => continue,
        };
        if t[k + 1] != Token::Mark('(') || !outside(k) {
            continue;
        }
        let args = arguments(&t, k + 1);
        // (Its own unit's: not `self.unit`'s, a weapon's.)
        if args.first().is_none_or(|&(a, b)| b != a + 1 || !is_name(&t[a], "self")) {
            continue;
        }
        let bone = args.get(1).and_then(|&(a, b)| match &t[a] {
            Token::Str(s) if b == a + 1 => Some(s.clone()),
            _ => (a..b).any(|j| is_name(&t[j], "BuildAttachBone")).then(|| "@BuildAttachBone".to_string()),
        });
        let (axis, rest) = match kind {
            Kind::Rotator => {
                let axis = args.get(2).and_then(|&(a, _)| match &t[a] {
                    Token::Str(s) => match s.to_ascii_lowercase().as_str() {
                        "x" => Some(0),
                        "y" => Some(1),
                        "z" => Some(2),
                        _ => None,
                    },
                    _ => None,
                });
                let Some(axis) = axis else { continue };
                (axis, 3)
            }
            Kind::Slider => (0, 2),
            Kind::Animator => (0, 1),
        };
        let numbers = args.iter().skip(rest).map(|&(a, b)| number(a, b)).collect();
        manips.push(Manip { kind, bone, axis, path: kept_as(&t, k), args: numbers, at: k });
    }
    // Where they are kept, and the tables of them gone through in loops.
    let by_path = |path: &str| -> Vec<usize> { manips.iter().enumerate().filter(|(_, m)| m.path.as_deref() == Some(path)).map(|(i, _)| i).collect() };
    let in_group = |group: &str| -> Vec<usize> {
        let prefix = format!("{group}.");
        manips.iter().enumerate().filter(|(_, m)| m.path.as_deref().is_some_and(|p| p.starts_with(&prefix) && !p[prefix.len()..].contains('.'))).map(|(i, _)| i).collect()
    };
    let mut groups: Vec<(String, String, usize, usize)> = Vec::new();
    for k in 0..t.len().saturating_sub(4) {
        if is_name(&t[k], "for")
            && let (Token::Name(_), Token::Mark(','), Token::Name(var), true) = (&t[k + 1], &t[k + 2], &t[k + 3], is_name(&t[k + 4], "in"))
            && let Some(d) = (k + 5..t.len()).find(|&j| is_name(&t[j], "do"))
        {
            let path = dotted(&t, k + 5, d);
            if let Some(path) = path {
                groups.push((var.clone(), path, k, block_end(&t, d)));
            }
        }
    }
    // The methods called on them.
    let mut calls: Vec<Call> = Vec::new();
    for k in 2..t.len().saturating_sub(1) {
        let Token::Name(method) = &t[k] else { continue };
        if !matches!(method.as_str(), "SetTargetSpeed" | "SetAccel" | "SetSpeed" | "SetGoal" | "SetSpinDown" | "SetRate" | "PlayAnim") || t[k - 1] != Token::Mark(':') || t[k + 1] != Token::Mark('(') || !outside(k) {
            continue;
        }
        let manips_of = match receiver(&t, k - 2) {
            Receiver::Made(at) => manips.iter().position(|m| m.at == at).into_iter().collect(),
            Receiver::Path(path) => match groups.iter().filter(|g| g.0 == path && k > g.2 && k < g.3).max_by_key(|g| g.2) {
                Some((_, group, _, _)) => in_group(group),
                None => by_path(&path),
            },
            Receiver::Unknown => Vec::new(),
        };
        if manips_of.is_empty() {
            continue;
        }
        let function = function_at(k);
        calls.push(Call { manips: manips_of, method: method.clone(), args: arguments(&t, k + 1), context: function.and_then(|f| contexts[f]), in_loop: loop_at(k, function), function, at: k });
    }
    let mut out = Motions { base, ..Motions::default() };
    for (i, m) in manips.iter().enumerate() {
        let (Some(bone), Some(made_in)) = (m.bone.clone(), context_at(m.at)) else {
            if m.kind == Kind::Animator {
                plays(&t, &calls, i, &number, &mut out.plays);
            }
            continue;
        };
        let own: Vec<&Call> = calls.iter().filter(|c| c.manips.contains(&i)).collect();
        match m.kind {
            Kind::Rotator => out.spins.extend(spin(&t, &loops, m, &bone, made_in, &own, &number)),
            Kind::Slider => out.slides.extend(slide(m, &bone, made_in, &own, &number)),
            Kind::Animator => plays(&t, &calls, i, &number, &mut out.plays),
        }
    }
    out
}

/// Each function's context (see the module's notes): its handler's; else its state's, where a
/// handler changes into it (else where what leads to it does: a state a handler changes into from
/// `OnCreate` and from `OnStopBeingBuilt` both is the unit's making, one it changes into from
/// `OnStopBeingBuilt` and from the making's state is its work); else that of what calls or forks
/// it. What runs in more than one takes the first of making, working, building, stopping.
fn contexts(t: &[Token], funs: &[Fun], states: &[(String, usize, usize)], function_at: &dyn Fn(usize) -> Option<usize>) -> Vec<Option<Context>> {
    let fixed: Vec<Option<Context>> = funs
        .iter()
        .map(|f| match f.state {
            Some(s) if stopping_state(&states[s].0) => Some(Context::Stopping),
            Some(_) if f.name == "Main" => None,
            _ => handler(&f.name),
        })
        .collect();
    let mut of: Vec<Option<Context>> = fixed.clone();
    let mut state_of: Vec<Option<Context>> = states.iter().map(|s| stopping_state(&s.0).then_some(Context::Stopping)).collect();
    // (Where the script changes into a state, and where it calls or forks a function of its own.)
    let mut changes: Vec<(usize, usize)> = Vec::new();
    let mut references: Vec<(usize, usize)> = Vec::new();
    for k in 0..t.len().saturating_sub(2) {
        if is_name(&t[k], "self") && matches!(t[k + 1], Token::Mark('.') | Token::Mark(':')) && let Token::Name(name) = &t[k + 2] {
            if k >= 4 && is_name(&t[k - 4], "ChangeState") && t[k - 3] == Token::Mark('(') {
                if let Some(s) = states.iter().position(|st| &st.0 == name) {
                    changes.push((k, s));
                }
            } else if let Some(f) = funs.iter().position(|f| &f.name == name && f.state.is_none()) {
                references.push((k, f));
            }
        }
    }
    // The context of what is reached from `sites` (handlers' first, then the rest's).
    let reached = |sites: &mut dyn Iterator<Item = usize>, of: &[Option<Context>], own: Option<usize>| -> Option<Context> {
        let (mut direct, mut derived): (Option<Context>, Option<Context>) = (None, None);
        for k in sites {
            let Some(f) = function_at(k).filter(|&f| Some(f) != own) else { continue };
            let Some(c) = of[f] else { continue };
            let slot = if fixed[f].is_some() { &mut direct } else { &mut derived };
            if slot.is_none_or(|old| c < old) {
                *slot = Some(c);
            }
        }
        direct.or(derived)
    };
    for _ in 0..12 {
        let mut changed = false;
        for s in 0..states.len() {
            if stopping_state(&states[s].0) {
                continue;
            }
            let new = reached(&mut changes.iter().filter(|c| c.1 == s).map(|c| c.0), &of, None);
            if new.is_some() && new != state_of[s] {
                state_of[s] = new;
                changed = true;
            }
        }
        for f in 0..funs.len() {
            if fixed[f].is_some() {
                continue;
            }
            let new = match funs[f].state {
                Some(s) => state_of[s],
                None => reached(&mut references.iter().filter(|r| r.1 == f).map(|r| r.0), &of, Some(f)),
            };
            if new.is_some() && new != of[f] {
                of[f] = new;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    of
}

/// The class the unit's is made of: the last name in `Class(...)` (`TMassCollectionUnit`, or
/// `import('/lua/seraphimunits.lua').SEnergyCreationUnit`'s).
fn base_class(t: &[Token]) -> String {
    let Some(k) = (0..t.len().saturating_sub(1)).find(|&k| is_name(&t[k], "Class") && t[k + 1] == Token::Mark('(')) else { return String::new() };
    let close = matching(t, k + 1);
    (k + 2..close).rev().find_map(|j| match &t[j] {
        Token::Name(n) => Some(n.clone()),
        _ => None,
    }).unwrap_or_default()
}

/// The index of the bracket closing the one at `open` (`(`, `{`), or the end.
fn matching(t: &[Token], open: usize) -> usize {
    let (o, c) = match t[open] {
        Token::Mark('{') => ('{', '}'),
        Token::Mark('[') => ('[', ']'),
        _ => ('(', ')'),
    };
    let mut depth = 0i32;
    for (k, tok) in t.iter().enumerate().skip(open) {
        if *tok == Token::Mark(o) {
            depth += 1;
        } else if *tok == Token::Mark(c) {
            depth -= 1;
            if depth == 0 {
                return k;
            }
        }
    }
    t.len()
}

/// A call's arguments (the call's `(` at `open`): each one's token range, split at its own commas.
fn arguments(t: &[Token], open: usize) -> Vec<(usize, usize)> {
    let close = matching(t, open);
    let mut out = Vec::new();
    let (mut from, mut depth) = (open + 1, 0i32);
    for (k, tok) in t.iter().enumerate().take(close).skip(open + 1) {
        match tok {
            Token::Mark('(') | Token::Mark('{') | Token::Mark('[') => depth += 1,
            Token::Mark(')') | Token::Mark('}') | Token::Mark(']') => depth -= 1,
            Token::Mark(',') if depth == 0 => {
                out.push((from, k));
                from = k + 1;
            }
            _ => {}
        }
    }
    if from < close {
        out.push((from, close));
    }
    out
}

/// A dotted name (`self.Spinners`) filling `t[from..to]`, or what `pairs(...)` is given.
fn dotted(t: &[Token], from: usize, to: usize) -> Option<String> {
    if to > from + 2 && is_name(&t[from], "pairs") && t[from + 1] == Token::Mark('(') {
        return dotted(t, from + 2, matching(t, from + 1));
    }
    let mut out = String::new();
    for (j, tok) in t.iter().enumerate().take(to).skip(from) {
        match (tok, (j - from) % 2) {
            (Token::Name(n), 0) => out.push_str(n),
            (Token::Mark('.'), 1) => out.push('.'),
            _ => return None,
        }
    }
    (!out.is_empty() && !out.ends_with('.')).then_some(out)
}

/// Where the manipulator made at `k` (its `Create...` token) is kept: `self.<name>`, a member of a
/// table kept as `self.<table>` (`self.<table>.<name>`), or a local; none where nothing keeps it.
fn kept_as(t: &[Token], k: usize) -> Option<String> {
    if k < 2 || t[k - 1] != Token::Mark('=') {
        return None;
    }
    let Token::Name(name) = &t[k - 2] else { return None };
    if k >= 4 && t[k - 3] == Token::Mark('.') && is_name(&t[k - 4], "self") {
        return Some(format!("self.{name}"));
    }
    if k >= 3 && is_name(&t[k - 3], "local") {
        return Some(name.clone());
    }
    // (In a table's constructor: `self.<table> = { <name> = Create...(...), ... }`.)
    let mut depth = 0i32;
    for j in (0..k - 2).rev() {
        match t[j] {
            Token::Mark('}') | Token::Mark(')') => depth += 1,
            Token::Mark('(') => depth -= 1,
            Token::Mark('{') if depth == 0 => {
                if j >= 4 && t[j - 1] == Token::Mark('=') && t[j - 3] == Token::Mark('.') && is_name(&t[j - 4], "self")
                    && let Token::Name(table) = &t[j - 2]
                {
                    return Some(format!("self.{table}.{name}"));
                }
                return None;
            }
            Token::Mark('{') => depth -= 1,
            _ => {}
        }
    }
    Some(name.clone())
}

/// What a method is called on (its receiver ending at `end`).
enum Receiver {
    /// The manipulator made at this token (`CreateRotator(...):SetAccel(5)`).
    Made(usize),
    /// A kept one (`self.Rotator`, `v`).
    Path(String),
    Unknown,
}

fn receiver(t: &[Token], end: usize) -> Receiver {
    match &t[end] {
        Token::Mark(')') => {
            // Back to its `(`: a manipulator's making, or a call chained on another.
            let mut depth = 0i32;
            let mut open = None;
            for j in (0..=end).rev() {
                match t[j] {
                    Token::Mark(')') => depth += 1,
                    Token::Mark('(') => {
                        depth -= 1;
                        if depth == 0 {
                            open = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(open) = open.filter(|&o| o >= 1) else { return Receiver::Unknown };
            match &t[open - 1] {
                Token::Name(n) if matches!(n.as_str(), "CreateRotator" | "CreateSlider" | "CreateAnimator") => Receiver::Made(open - 1),
                Token::Name(_) if open >= 3 && t[open - 2] == Token::Mark(':') => receiver(t, open - 3),
                _ => Receiver::Unknown,
            }
        }
        Token::Name(_) => {
            let mut from = end;
            while from >= 2 && t[from - 1] == Token::Mark('.') && matches!(t[from - 2], Token::Name(_)) {
                from -= 2;
            }
            match dotted(t, from, end + 1) {
                Some(path) if from == 0 || !matches!(t[from - 1], Token::Mark(':') | Token::Mark('.')) => Receiver::Path(path),
                _ => Receiver::Unknown,
            }
        }
        _ => Receiver::Unknown,
    }
}

/// Numbers as a script works them out (see the module's notes).
struct Numbers<'a> {
    t: &'a [Token],
    kept: &'a BTreeMap<String, usize>,
}

impl Numbers<'_> {
    /// The number `t[from..to]` is, all of it.
    fn range(&self, from: usize, to: usize) -> Option<f64> {
        let mut p = Parser { n: self, at: from, end: to, depth: 0 };
        let v = p.sum();
        (p.at == to).then_some(v).flatten()
    }
}

struct Parser<'a, 'b> {
    n: &'b Numbers<'a>,
    at: usize,
    end: usize,
    depth: u8,
}

impl Parser<'_, '_> {
    fn mark(&self, c: char) -> bool {
        self.at < self.end && self.n.t[self.at] == Token::Mark(c)
    }

    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product();
        while self.mark('+') || self.mark('-') {
            let minus = self.mark('-');
            self.at += 1;
            let w = self.product();
            v = match (v, w) {
                (Some(a), Some(b)) => Some(if minus { a - b } else { a + b }),
                _ => None,
            };
        }
        v
    }

    /// (A factor it does not know is taken as 1: a random way round, `* num`.)
    fn product(&mut self) -> Option<f64> {
        let mut v = self.unary();
        while self.mark('*') || self.mark('/') {
            let divide = self.mark('/');
            self.at += 1;
            let w = self.unary();
            v = match (v, w) {
                (Some(a), Some(b)) if divide => (b != 0.0).then(|| a / b),
                (Some(a), Some(b)) => Some(a * b),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            };
        }
        v
    }

    fn unary(&mut self) -> Option<f64> {
        if self.mark('-') {
            self.at += 1;
            return self.unary().map(|v| -v);
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        let t = self.n.t;
        if self.at >= self.end {
            return None;
        }
        match &t[self.at] {
            Token::Num(s) => {
                self.at += 1;
                s.parse().ok()
            }
            Token::Mark('(') => {
                let close = matching(t, self.at);
                let inner = self.n.range(self.at + 1, close.min(self.end));
                self.at = (close + 1).min(self.end);
                inner
            }
            Token::Name(n) if n == "Random" && self.at + 1 < self.end && t[self.at + 1] == Token::Mark('(') => {
                let args = arguments(t, self.at + 1);
                self.at = (matching(t, self.at + 1) + 1).min(self.end);
                match args.as_slice() {
                    [] => Some(0.5),
                    [(a, b), (c, d)] => Some((self.n.range(*a, *b)? + self.n.range(*c, *d)?) / 2.0),
                    _ => None,
                }
            }
            Token::Name(_) => {
                // A dotted name, a value kept (`self.Goal`), or a call: worked out where it is kept.
                let from = self.at;
                self.at += 1;
                while self.at + 1 < self.end && matches!(t[self.at], Token::Mark('.') | Token::Mark(':')) && matches!(t[self.at + 1], Token::Name(_)) {
                    self.at += 2;
                }
                if self.mark('(') {
                    self.at = (matching(t, self.at) + 1).min(self.end);
                    return None;
                }
                match (&t[from], self.at - from) {
                    (Token::Name(s), 3) if s == "self" => {
                        let Token::Name(name) = &t[from + 2] else { return None };
                        let at = *self.n.kept.get(name)?;
                        if self.depth >= 4 {
                            return None;
                        }
                        let mut inner = Parser { n: self.n, at, end: t.len(), depth: self.depth + 1 };
                        inner.sum()
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// A rotator's spin, where it has one (see the module's notes): its speed at work (the last one
/// set where its unit starts working or building, else as it is made), its acceleration, and
/// whether it swings.
fn spin(t: &[Token], loops: &[(usize, usize)], m: &Manip, bone: &str, made_in: Context, own: &[&Call], number: &dyn Fn(usize, usize) -> Option<f64>) -> Option<Spin> {
    let arg = |k: usize| m.args.get(k).copied().flatten();
    let first = |c: &Call| c.args.first().and_then(|&(a, b)| number(a, b));
    // (Its own call: `CreateRotator(unit, bone, axis, goal, speed, accel, goalspeed)`.)
    let made_accel = arg(2);
    let accel = own
        .iter()
        .filter(|c| c.method == "SetAccel" && matches!(c.context, Some(Context::Working | Context::Building)))
        .filter_map(|c| first(c))
        .next_back()
        .or(made_accel)
        .or_else(|| own.iter().filter(|c| c.method == "SetAccel" && c.context == Some(Context::Made)).filter_map(|c| first(c)).next_back())
        .unwrap_or(0.0)
        .abs();
    // Its speeds where it works or builds: the goal speed of a rotator made there, and those set.
    let mut working: Vec<(usize, f64, Context, Option<usize>)> = Vec::new();
    if matches!(made_in, Context::Working | Context::Building)
        && let Some(v) = arg(3).filter(|v| *v != 0.0)
    {
        working.push((m.at, v, made_in, None));
    }
    for c in own.iter().filter(|c| c.method == "SetTargetSpeed" && matches!(c.context, Some(Context::Working | Context::Building))) {
        if let Some(v) = first(c) {
            working.push((c.at, v, c.context.unwrap_or(Context::Working), c.in_loop));
        }
    }
    working.sort_by_key(|w| w.0);
    // Both ways in one loop: a swing.
    for &(_, v, context, in_loop) in &working {
        let Some(l) = in_loop else { continue };
        let ways: Vec<f64> = working.iter().filter(|w| w.3 == Some(l) && w.1 != 0.0).map(|w| w.1).collect();
        if ways.iter().any(|w| *w > 0.0) && ways.iter().any(|w| *w < 0.0) && v != 0.0 {
            let most = ways.iter().fold(0.0f64, |a, w| a.max(w.abs()));
            let end = loops.iter().find(|lp| lp.0 == l).map_or(l, |lp| lp.1);
            let pause = (l..end.min(t.len().saturating_sub(2)))
                .filter(|&k| is_name(&t[k], "WaitSeconds") && t[k + 1] == Token::Mark('('))
                .filter_map(|k| arguments(t, k + 1).first().and_then(|&(a, b)| number(a, b)))
                .fold(0.0f64, f64::max);
            return Some(Spin { bone: bone.to_string(), axis: m.axis, speed: most.copysign(v), accel, when: context.when()?, sway: Some(pause) });
        }
    }
    if let Some(&(_, v, context, _)) = working.iter().rfind(|w| w.1 != 0.0) {
        return Some(Spin { bone: bone.to_string(), axis: m.axis, speed: v, accel, when: context.when()?, sway: None });
    }
    // Made spinning: its goal speed, or a speed it keeps (no acceleration to change it).
    if made_in == Context::Made {
        let set = own.iter().filter(|c| c.method == "SetTargetSpeed" && c.context == Some(Context::Made)).filter_map(|c| first(c)).next_back();
        let speed = match set {
            Some(v) => v,
            None => arg(3).filter(|v| *v != 0.0).or_else(|| arg(1).filter(|_| made_accel.unwrap_or(0.0) == 0.0)).unwrap_or(0.0),
        };
        if speed != 0.0 {
            return Some(Spin { bone: bone.to_string(), axis: m.axis, speed, accel, when: When::Made, sway: None });
        }
    }
    None
}

/// A slider's slide, where it has one (see the module's notes).
fn slide(m: &Manip, bone: &str, made_in: Context, own: &[&Call], number: &dyn Fn(usize, usize) -> Option<f64>) -> Option<Slide> {
    let place = |c: &Call| -> Option<[f64; 3]> {
        let v: Vec<f64> = c.args.iter().map(|&(a, b)| number(a, b).unwrap_or(0.0)).collect();
        (v.len() == 3).then(|| [v[0], v[1], v[2]])
    };
    let speed_in = |function: Option<usize>| -> Option<f64> { own.iter().filter(|c| c.method == "SetSpeed" && c.function == function).filter_map(|c| c.args.first().and_then(|&(a, b)| number(a, b))).next() };
    // (Its own call: `CreateSlider(unit, bone, x, y, z, speed)`.)
    let made_goal = (m.args.len() >= 3).then(|| [m.args[0].unwrap_or(0.0), m.args[1].unwrap_or(0.0), m.args[2].unwrap_or(0.0)]);
    let made_speed = m.args.get(3).copied().flatten();
    let mut made = if made_in == Context::Made { made_goal } else { None };
    if let Some(p) = own.iter().filter(|c| c.method == "SetGoal" && c.context == Some(Context::Made)).filter_map(|c| place(c)).next_back() {
        made = Some(p);
    }
    let moving: Vec<&&Call> = own.iter().filter(|c| c.method == "SetGoal" && matches!(c.context, Some(Context::Working | Context::Building))).collect();
    let back = own.iter().filter(|c| c.method == "SetSpeed" && c.context == Some(Context::Stopping)).filter_map(|c| c.args.first().and_then(|&(a, b)| number(a, b))).next();
    let made_at_work = matches!(made_in, Context::Working | Context::Building).then_some(made_goal).flatten();
    if moving.is_empty() && made_at_work.is_none() {
        return made.map(|p| Slide { bone: bone.to_string(), made: Some(p), goals: Vec::new(), shuttle: false, speed: 0.0, back: 0.0, when: When::Made });
    }
    // Places set in one loop: shuttling between them; else the last place it goes to.
    let looped = moving.iter().find_map(|c| c.in_loop);
    let (goals, shuttle, context, function) = match looped {
        Some(l) => {
            let in_it: Vec<&&&Call> = moving.iter().filter(|c| c.in_loop == Some(l)).collect();
            let mut goals: Vec<[f64; 3]> = Vec::new();
            for c in &in_it {
                if let Some(p) = place(c)
                    && goals.last() != Some(&p)
                {
                    goals.push(p);
                }
            }
            (goals, true, in_it[0].context.unwrap_or(Context::Working), in_it[0].function)
        }
        None => match moving.last() {
            Some(c) => (place(c).into_iter().collect(), false, c.context.unwrap_or(Context::Working), c.function),
            None => (made_at_work.into_iter().collect(), false, made_in, None),
        },
    };
    if goals.is_empty() {
        return None;
    }
    let speed = speed_in(function).or(made_speed).unwrap_or(0.0).max(0.0);
    Some(Slide { bone: bone.to_string(), made, goals, shuttle, speed, back: back.unwrap_or(speed).max(0.0), when: context.when()? })
}

/// The animations the animator `i` is made to play (`PlayAnim`), where its unit works or builds:
/// which, whether round and round, and how fast.
fn plays(t: &[Token], calls: &[Call], i: usize, number: &dyn Fn(usize, usize) -> Option<f64>, out: &mut Vec<Play>) {
    for c in calls.iter().filter(|c| c.manips.contains(&i) && c.method == "PlayAnim") {
        let Some(when) = c.context.and_then(Context::when).filter(|w| *w != When::Made) else { continue };
        let Some(&(a, b)) = c.args.first() else { continue };
        let animation = match &t[a] {
            Token::Str(path) if b == a + 1 => path.trim_start_matches('/').to_ascii_lowercase(),
            _ => match (a..b).rev().find_map(|j| match &t[j] {
                Token::Name(n) => Some(n.clone()),
                _ => None,
            }) {
                Some(key) => key,
                None => continue,
            },
        };
        let looped = c.args.get(1).is_some_and(|&(a, b)| b == a + 1 && is_name(&t[a], "true")) || c.in_loop.is_some();
        // Its rate: chained on it, or set on its animator next in the same function.
        let rate = calls
            .iter()
            .filter(|r| r.method == "SetRate" && r.manips.contains(&i) && r.function == c.function && r.at > c.at)
            .min_by_key(|r| r.at)
            .and_then(|r| r.args.first().and_then(|&(a, b)| number(a, b)))
            .filter(|r| *r > 0.0)
            .unwrap_or(1.0);
        // (Played once and then again in a loop: round and round.)
        match out.iter_mut().find(|p| p.animation == animation && p.when == when) {
            Some(p) => p.looped |= looped,
            None => out.push(Play { animation, looped, rate, when }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A power plant of our own making: panels slid out once it is finished, two drums spun up
    /// after them, all of it kept in tables and gone through in loops; a speed set as it closes
    /// (damaged) is no working speed.
    const PLANT: &str = r#"
local Base = import('/lua/somewhere.lua').PowerBase
Plant = Class(Base) {
    OnCreate = function(self)
        Base.OnCreate(self)
        self.Panels = {
            Left = CreateSlider(self, 'PanelL'),
            Right = CreateSlider(self, 'PanelR'),
        }
        self.Drums = {
            Inner = CreateRotator(self, 'DrumA', 'y', nil, 0, 45, 300):SetTargetSpeed(0),
            Outer = CreateRotator(self, 'DrumB', 'z', nil, 0, 20, 300):SetTargetSpeed(0),
        }
    end,
    OnStopBeingBuilt = function(self, builder, layer)
        Base.OnStopBeingBuilt(self, builder, layer)
        ChangeState(self, self.SpinUp)
    end,
    OnDamage = function(self, instigator, amount)
        ChangeState(self, self.Shut)
    end,
    SpinUp = State {
        Main = function(self)
            self.Panels.Left:SetGoal(-2, 0, 0)
            self.Panels.Left:SetSpeed(4)
            self.Panels.Right:SetGoal(2, 0, 0)
            self.Panels.Right:SetSpeed(4)
            for k, v in self.Panels do
                WaitFor(v)
            end
            self.Drums.Inner:SetTargetSpeed(120)
            self.Drums.Outer:SetTargetSpeed(-60)
        end,
    },
    Shut = State {
        Main = function(self)
            for k, v in self.Drums do
                v:SetTargetSpeed(500)
            end
            for k, v in self.Panels do
                v:SetGoal(0, 0, 0)
                v:SetSpeed(16)
            end
        end,
    },
}
"#;

    #[test]
    fn a_plants_panels_slide_out_and_its_drums_spin_once_it_is_built() {
        let m = motions(PLANT);
        assert_eq!(m.base, "Base");
        assert_eq!(m.spins, [
            Spin { bone: "DrumA".into(), axis: 1, speed: 120.0, accel: 45.0, when: When::Working, sway: None },
            Spin { bone: "DrumB".into(), axis: 2, speed: -60.0, accel: 20.0, when: When::Working, sway: None },
        ]);
        assert_eq!(m.slides.len(), 2);
        let left = &m.slides[0];
        assert_eq!((left.bone.as_str(), left.goals.as_slice(), left.shuttle, left.speed, left.back, left.when, left.made), ("PanelL", &[[-2.0, 0.0, 0.0]][..], false, 4.0, 16.0, When::Working, None));
        assert!(m.plays.is_empty());
    }

    /// A radar of our own: dishes switched on and off with its intel, one spun up by setting
    /// its speeds apart, one made with its numbers in its own call, a third swinging to and fro
    /// (both ways in a loop, a pause at random), started from a thread its handler forks.
    const RADAR: &str = r#"
Radar = Class(RadarBase) {
    OnIntelEnabled = function(self)
        RadarBase.OnIntelEnabled(self)
        if not self.Top then
            self.Top = CreateRotator(self, 'Top', 'z')
            self.Trash:Add(self.Top)
        end
        self.Top:SetTargetSpeed(25)
        self.Top:SetAccel(8)
        self.Trash:Add(CreateRotator(self, 'Ring', 'x', nil, 0, 12, 70 + Random(0, 10) * dir))
        self.Swinger = self:ForkThread(self.Nod)
    end,
    OnIntelDisabled = function(self)
        RadarBase.OnIntelDisabled(self)
        self.Top:SetTargetSpeed(0)
        self.Top:SetAccel(40)
        KillThread(self.Swinger)
    end,
    Nod = function(self)
        local dish = CreateRotator(self, 'Dish', 'x')
        dish:SetAccel(4)
        while true do
            dish:SetTargetSpeed(-12)
            WaitFor(dish)
            dish:SetTargetSpeed(0)
            WaitFor(dish)
            if Random() < 0.3 then WaitSeconds(2) end
            dish:SetTargetSpeed(12)
            WaitFor(dish)
            dish:SetTargetSpeed(0)
            WaitFor(dish)
        end
    end,
}
"#;

    #[test]
    fn a_radars_dishes_spin_and_swing_while_it_watches() {
        let m = motions(RADAR);
        assert_eq!(m.spins, [
            Spin { bone: "Top".into(), axis: 2, speed: 25.0, accel: 8.0, when: When::Working, sway: None },
            Spin { bone: "Ring".into(), axis: 0, speed: 75.0, accel: 12.0, when: When::Working, sway: None },
            Spin { bone: "Dish".into(), axis: 0, speed: -12.0, accel: 4.0, when: When::Working, sway: Some(2.0) },
        ]);
    }

    /// An extractor of our own: its pump played round and round once built (a value kept, and the
    /// rate set on the next line), a hatch played once where it is switched on, a factory's arm
    /// shuttling while it builds, a part set at once as it is made, and a weapon's turning left
    /// to its weapon.
    const PUMP: &str = r#"
Pump = Class(import('/lua/somewhere.lua').PumpBase) {
    Weapons = {
        Gun = Class(GunBase) {
            PlayFxRackReload = function(self)
                self.Spinner = CreateRotator(self.unit, 'Barrel', 'z', nil, 200, 100, 200)
            end,
        },
    },
    OnCreate = function(self)
        PumpBase.OnCreate(self)
        self.Plug = CreateSlider(self, 'Plug')
        ChangeState(self, self.Closing)
        self.Pod = CreateRotator(self, 'Pod', 'y', nil, 7, 0, 0)
    end,
    Closing = State {
        Main = function(self)
            self.Plug:SetGoal(0, -3, 0)
            self.Plug:SetSpeed(-1)
        end,
    },
    PlayActiveAnimation = function(self)
        local bp = self:GetBlueprint().Display
        self.Pumping = CreateAnimator(self)
        self.Pumping:PlayAnim(bp.AnimationPump, true)
        self.Pumping:SetRate(self.Rate)
    end,
    OnProductionUnpaused = function(self)
        self.Hatch = CreateAnimator(self):PlayAnim(self:GetBlueprint().Display.AnimationHatch):SetRate(2)
    end,
    OnProductionPaused = function(self)
        self.Pumping:SetRate(0)
    end,
    StartArmsMoving = function(self)
        self.Arm = CreateSlider(self, 'Arm')
        self.Trash:Add(self.Arm)
    end,
    MovingArmsThread = function(self)
        while true do
            self.Arm:SetGoal(0, 0, 10)
            self.Arm:SetSpeed(5)
            WaitFor(self.Arm)
            self.Arm:SetGoal(0, 0, -10)
            WaitFor(self.Arm)
        end
    end,
    StopArmsMoving = function(self)
        self.Arm:SetGoal(0, 0, 0)
        self.Arm:SetSpeed(20)
    end,
    Rate = 0.5,
}
"#;

    #[test]
    fn a_pumps_animations_and_a_factorys_arm() {
        let m = motions(PUMP);
        assert_eq!(m.base, "PumpBase");
        assert_eq!(m.spins, [Spin { bone: "Pod".into(), axis: 1, speed: 7.0, accel: 0.0, when: When::Made, sway: None }], "the gun's barrel is its weapon's");
        assert_eq!(m.plays, [
            Play { animation: "AnimationPump".into(), looped: true, rate: 1.0, when: When::Working },
            Play { animation: "AnimationHatch".into(), looped: false, rate: 2.0, when: When::Working },
        ]);
        let plug = m.slides.iter().find(|s| s.bone == "Plug").unwrap();
        assert_eq!((plug.made, plug.goals.is_empty(), plug.when), (Some([0.0, -3.0, 0.0]), true, When::Made));
        let arm = m.slides.iter().find(|s| s.bone == "Arm").unwrap();
        assert_eq!((arm.goals.as_slice(), arm.shuttle, arm.speed, arm.back, arm.when), (&[[0.0, 0.0, 10.0], [0.0, 0.0, -10.0]][..], true, 5.0, 20.0, When::Building));
    }

    /// Numbers as the scripts work them out: a value kept, a random one's middle, a factor not
    /// known taken as 1.
    #[test]
    fn numbers_are_worked_out_where_they_can_be() {
        let t = tokens("self.Goal = Random(100, 200) x = -self.Goal + 3 * dir y = (4 + 2) / 3 z = speed");
        let mut kept = BTreeMap::new();
        kept.insert("Goal".to_string(), 4);
        let n = Numbers { t: &t, kept: &kept };
        let at = |name: &str| t.iter().position(|tok| is_name(tok, name)).unwrap() + 2;
        let end = |from: usize| (from..t.len()).find(|&k| matches!(&t[k], Token::Name(n) if n.len() == 1)).unwrap_or(t.len());
        assert_eq!(n.range(at("x"), end(at("x"))), Some(-147.0));
        assert_eq!(n.range(at("y"), end(at("y"))), Some(2.0));
        assert_eq!(n.range(at("z"), t.len()), None, "a speed it does not know");
    }
}
