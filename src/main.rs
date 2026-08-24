// asciiquarium-rs: an aquarium animation in ASCII art.
//
// A Rust port (2026) of asciiquarium, whose art is mostly by Joan Stark with
// later additions backported by Claudio Matsuoka.
// Copyright (C) 2003 Kirk Baucom <kbaucom@schizoid.com> (original program)
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation; either version 2 of the License, or (at your option)
// any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
// You should have received a copy of the GNU General Public License along with
// this program; if not, see the LICENSE file at the repository root.

//! asciiquarium, in Rust. See README.md for the port notes.
//!
//! A hand-rolled compositor (`render`), an entity model (`entity`), byte-exact
//! art (`art`), and the spawners (`spawn`), driven by a crossterm event loop
//! that doubles as the frame clock.

use asciiquarium::{entity, render, spawn};

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType, DisableLineWrap, EnableLineWrap},
};
use rand::{Rng, RngExt};

use entity::{Entity, EntityType, OnDeath};

/// An aquarium animation in ASCII art.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Classic mode: use the original art set only.
    #[arg(short = 'c', long)]
    classic: bool,

    /// Exit after this many seconds.
    #[arg(long, value_name = "SECONDS")]
    duration_seconds: Option<NonZeroU64>,

    /// Exit on any key instead of reserving keys for controls.
    #[arg(long)]
    exit_on_any_key: bool,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let mut term = TerminalGuard::enter()?;
    run(
        &mut term.out,
        cli.classic,
        cli.duration_seconds
            .map(|seconds| Duration::from_secs(seconds.get())),
        cli.exit_on_any_key,
    )
}

fn run(
    out: &mut impl Write,
    classic: bool,
    duration: Option<Duration>,
    exit_on_any_key: bool,
) -> io::Result<()> {
    // A termination signal (SIGTERM/SIGHUP, or the console window closing)
    // restores the terminal and exits from the handler thread directly. It
    // must not just set a flag for the main loop: when the terminal dies out
    // from under us (pty master closed), crossterm's event poll spins on the
    // EOF'd tty without returning, so the main loop never runs again and a
    // flag would never be checked -- the process would survive SIGTERM/SIGHUP
    // and busy-loop until SIGKILL. (Ctrl-C arrives as a key in raw mode and
    // is handled in the loop below.) ctrlc abstracts the per-platform signal
    // vs console-handler details.
    ctrlc::set_handler(|| {
        restore_terminal(&mut io::stdout());
        std::process::exit(0);
    })
    .map_err(io::Error::other)?;

    let mut rng = rand::rng();
    let (mut w, mut h) = terminal::size()?;
    let mut screen = render::Screen::new(w, h);
    let mut tick: u64 = 0;
    let mut entities = build_scene(w, h, classic, &mut rng);
    let mut paused = false;
    let started = Instant::now();

    loop {
        if duration.is_some_and(|duration| started.elapsed() >= duration) {
            return Ok(());
        }

        // poll() is both the input reader and the ~10fps frame clock: it blocks
        // up to 100ms for an event, exactly like the original's halfdelay(1).
        let poll_duration = duration
            .map(|duration| duration.saturating_sub(started.elapsed()))
            .unwrap_or(Duration::from_millis(100))
            .min(Duration::from_millis(100));
        if event::poll(poll_duration)? {
            match event::read()? {
                Event::Key(k) => {
                    if exit_on_any_key {
                        return Ok(());
                    }
                    match k.code {
                        // Raw mode swallows SIGINT, so handle Ctrl-C / Ctrl-D here.
                        KeyCode::Char('c' | 'd') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(());
                        }
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('p' | ' ') => paused = !paused,
                        KeyCode::Char('r') => entities = build_scene(w, h, classic, &mut rng),
                        _ => {}
                    }
                }
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                    screen = render::Screen::new(w, h);
                    entities = build_scene(w, h, classic, &mut rng);
                }
                _ => {}
            }
        }

        if !paused {
            advance(&mut entities, tick, w, h, classic, &mut rng);
            tick += 1;
        }

        // Draw back-to-front: higher z first, lower z last (on top).
        screen.clear();
        entities.sort_by_key(|e| std::cmp::Reverse(e.z));
        for e in &entities {
            let f = e.current();
            screen.blit(
                e.x.round() as i32,
                e.y.round() as i32,
                &f.shape,
                &f.mask,
                e.default_color,
                e.auto_trans,
                e.trans,
            );
        }
        screen.render(out)?;
    }
}

/// Advance the world one frame: move, emit bubbles, resolve collisions, then
/// remove the dead and run their death actions (respawns and the roaming
/// special's successor).
fn advance(
    entities: &mut Vec<Entity>,
    tick: u64,
    w: u16,
    h: u16,
    classic: bool,
    rng: &mut impl Rng,
) {
    for e in entities.iter_mut() {
        e.update(tick, w, h);
    }

    let mut spawns: Vec<Entity> = Vec::new();

    // Fish occasionally blow a bubble (original: ~2% chance per tick).
    for e in entities.iter() {
        if e.etype == EntityType::Fish && e.alive && rng.random_range(0..100) > 97 {
            spawns.push(spawn::bubble(e));
        }
    }

    resolve_collisions(entities, &mut spawns);

    // When the shark dies, its teeth (the collision proxy) go with it.
    if entities
        .iter()
        .any(|e| e.etype == EntityType::Shark && !e.alive)
    {
        for e in entities.iter_mut() {
            if e.etype == EntityType::Teeth {
                e.alive = false;
            }
        }
    }

    if entities
        .iter()
        .any(|e| e.etype == EntityType::FishHook && !e.alive)
    {
        for e in entities.iter_mut() {
            if matches!(e.etype, EntityType::FishLine | EntityType::HookPoint) {
                e.alive = false;
            }
        }
    }

    if entities
        .iter()
        .any(|e| e.etype == EntityType::Dolphin && !e.alive)
    {
        for e in entities.iter_mut() {
            if e.etype == EntityType::Dolphin && e.alive {
                e.die_offscreen = true;
            }
        }
    }

    // Death actions: the port of Term::Animation's death_cb.
    for e in entities.iter().filter(|e| !e.alive) {
        match e.on_death {
            OnDeath::Fish => spawns.push(spawn::fish(w, h, classic, rng)),
            OnDeath::Seaweed => spawns.push(spawn::seaweed(w, h, tick, rng)),
            OnDeath::RandomObject => spawns.extend(spawn::random_object(w, h, classic, rng)),
            OnDeath::Nothing => {}
        }
    }

    entities.retain(|e| e.alive);
    entities.extend(spawns);
}

/// Cell-overlap collisions among physical entities. Two rules, as in the
/// original: a small fish meeting the shark's teeth is eaten (leaving a splat),
/// and a bubble meeting the waterline pops.
fn resolve_collisions(entities: &mut [Entity], spawns: &mut Vec<Entity>) {
    // Which entities occupy each cell.
    let mut occ: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, e) in entities.iter().enumerate() {
        if e.physical && e.alive {
            for c in e.cells() {
                occ.entry(c).or_default().push(i);
            }
        }
    }

    let mut eat: Vec<(usize, usize)> = Vec::new(); // (fish, teeth)
    let mut hooked: Vec<usize> = Vec::new();
    let mut pop: HashSet<usize> = HashSet::new(); // bubbles
    let mut seen_fish: HashSet<usize> = HashSet::new();

    for occupants in occ.values() {
        if occupants.len() < 2 {
            continue;
        }
        let has = |t: EntityType| occupants.iter().find(|&&i| entities[i].etype == t).copied();
        for &i in occupants {
            match entities[i].etype {
                EntityType::Fish => {
                    if !seen_fish.contains(&i) {
                        if let Some(t) = has(EntityType::Teeth) {
                            eat.push((i, t));
                            seen_fish.insert(i);
                        } else if has(EntityType::HookPoint).is_some() {
                            hooked.push(i);
                            seen_fish.insert(i);
                        }
                    }
                }
                EntityType::Bubble if has(EntityType::Waterline).is_some() => {
                    pop.insert(i);
                }
                _ => {}
            }
        }
    }

    for (fish, teeth) in eat {
        let (tx, ty, tz) = (
            entities[teeth].x.round() as i32,
            entities[teeth].y.round() as i32,
            entities[teeth].z,
        );
        spawns.push(spawn::splat(tx, ty, tz));
        entities[fish].alive = false;
    }
    for b in pop {
        entities[b].alive = false;
    }
    if !hooked.is_empty() {
        for e in entities.iter_mut() {
            if matches!(
                e.etype,
                EntityType::FishLine | EntityType::FishHook | EntityType::HookPoint
            ) {
                e.retracting = true;
            }
        }
        for fish in hooked {
            entities[fish].dx = 0.0;
            entities[fish].dy = -1.0;
            entities[fish].die_offscreen = true;
        }
    }
}

/// Build the whole scene: water, castle, seaweed, a screen-sized school of
/// fish, and one roaming special.
fn build_scene(w: u16, h: u16, classic: bool, rng: &mut impl Rng) -> Vec<Entity> {
    let mut entities = spawn::water(w);
    entities.push(spawn::castle(w, h));

    let seaweed_count = w as i32 / 15;
    for _ in 0..seaweed_count {
        entities.push(spawn::seaweed(w, h, 0, rng));
    }

    let fish_count = (h as i32 - 9).max(0) * w as i32 / 350;
    for _ in 0..fish_count {
        entities.push(spawn::fish(w, h, classic, rng));
    }

    entities.extend(spawn::random_object(w, h, classic, rng));
    entities
}

/// Puts the terminal into raw mode and gives the aquarium the original solid
/// black canvas. The surrounding ttyd theme is restored on exit.
struct TerminalGuard {
    out: io::Stdout,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = TerminalGuard { out: io::stdout() };
        // Disable auto-wrap: a full-width row must not wrap past the last
        // column, or it compounds with our newline and drifts the frame.
        execute!(
            guard.out,
            SetBackgroundColor(Color::Black),
            Clear(ClearType::All),
            cursor::MoveTo(0, 0),
            cursor::Hide,
            DisableLineWrap
        )?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal(&mut self.out);
    }
}

/// Undo TerminalGuard::enter. Shared by the guard's Drop (normal exits and
/// panics) and the termination-signal handler (which exits without unwinding).
/// Every step tolerates a dead terminal.
fn restore_terminal(out: &mut io::Stdout) {
    let _ = execute!(
        out,
        ResetColor,
        Clear(ClearType::All),
        cursor::MoveTo(0, 0),
        cursor::Show,
        EnableLineWrap
    );
    let _ = terminal::disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_run_options() {
        let cli = Cli::try_parse_from([
            "asciiquarium-rs",
            "--classic",
            "--duration-seconds",
            "3",
            "--exit-on-any-key",
        ])
        .unwrap();

        assert!(cli.classic);
        assert_eq!(cli.duration_seconds.unwrap().get(), 3);
        assert!(cli.exit_on_any_key);
    }

    #[test]
    fn rejects_zero_duration() {
        assert!(Cli::try_parse_from(["asciiquarium-rs", "--duration-seconds", "0"]).is_err());
    }
}
