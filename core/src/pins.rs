//! Pin geometry/lifecycle independent of native surfaces and image decoding.
use crate::{
    annotation::Bounds,
    capture::{Output, contains, intersect},
    i18n::{t, tf},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod mouse;
pub mod preferences;
fn enabled() -> bool {
    true
}
fn maximum_size() -> f64 {
    12000.
}

pub const ACTIVE_LIMIT: usize = 32;
pub const STORED_LIMIT: usize = 128;
pub const DEFAULT_GROUP: &str = "default";
pub const MAX_GROUPS: usize = 64;
pub const MAX_GROUP_NAME_LEN: usize = 128;
pub const GROUP_COLOR_PRESETS: &[(&str, &str)] = &[
    ("Red", "#ff5555"),
    ("Orange", "#ffb86c"),
    ("Yellow", "#f1fa8c"),
    ("Green", "#50fa7b"),
    ("Blue", "#5bc0eb"),
    ("Purple", "#bd93f9"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub active_pins: usize,
    pub closed_pins: usize,
    pub current: bool,
    pub default: bool,
}

pub fn group_display_name(name: &str) -> String {
    if name == DEFAULT_GROUP {
        t("Default")
    } else {
        name.to_owned()
    }
}

pub fn normalize_group_color(color: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = color.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if raw.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let valid = raw.len() == 7
        && raw.starts_with('#')
        && raw.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit);
    if !valid {
        return Err(t("Invalid pin group color"));
    }
    Ok(Some(raw.to_ascii_lowercase()))
}

fn validate_group_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(t("Group name cannot be empty"));
    }
    if trimmed.chars().count() > MAX_GROUP_NAME_LEN {
        return Err(t("The group name or group count exceeds its limit"));
    }
    Ok(trimmed.to_owned())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    #[serde(skip)]
    pub id: u64,
    #[serde(default)]
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_text: Option<String>,
    #[serde(default)]
    pub grayscale: bool,
    #[serde(default)]
    pub inverted: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub opacity: f64,
    #[serde(default = "enabled")]
    pub shadow: bool,
    #[serde(default = "enabled")]
    pub smooth: bool,
    #[serde(default = "maximum_size")]
    pub max_size: f64,
    pub group: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub through: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub closed_at: u64,
    #[serde(skip)]
    pub thumbnail_zoom: Option<f64>,
}
fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}
/// Snap a logical coordinate to the nearest physical pixel on `scale`.
pub fn snap_coord(x: f64, scale: f64) -> f64 {
    let s = if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    };
    (x * s + 0.5).floor() / s
}
impl Pin {
    pub fn maximum_zoom(&self) -> f64 {
        (self.max_size / self.width.max(self.height)).max(self.minimum_zoom())
    }
    pub fn minimum_zoom(&self) -> f64 {
        (24. / self.width.max(self.height)).clamp(0.00001, 1.)
    }
    pub fn rect(&self) -> Bounds {
        let minimum = self.minimum_zoom();
        let zoom = self.zoom.clamp(minimum, self.maximum_zoom());
        Bounds {
            x: self.x,
            y: self.y,
            width: if zoom == 1. {
                self.width
            } else {
                round(self.width * zoom).max(1.)
            },
            height: if zoom == 1. {
                self.height
            } else {
                round(self.height * zoom).max(1.)
            },
        }
    }
    pub fn valid(&self) -> bool {
        [
            self.x,
            self.y,
            self.width,
            self.height,
            self.zoom,
            self.opacity,
        ]
        .iter()
        .all(|x| x.is_finite() && x.abs() <= 100000.)
            && self
                .plain_text
                .as_ref()
                .is_none_or(|s| s.chars().count() <= crate::paste::MAX_TEXT)
            && self.width > 0.
            && self.max_size.is_finite()
            && (100. ..=100000.).contains(&self.max_size)
            && self.height > 0.
            && (0.00001..=100000.).contains(&self.zoom)
            && (0.1..=1.).contains(&self.opacity)
            && self.group.chars().count() <= 128
            && self
                .source
                .as_ref()
                .is_none_or(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    }
    pub fn edge(&self, p: [f64; 2]) -> [bool; 4] {
        let r = self.rect();
        if !contains(r, p) {
            return [false; 4];
        }
        let m = 7f64.min(r.width / 3.).min(r.height / 3.);
        [
            p[0] < r.x + m,
            p[1] < r.y + m,
            p[0] >= r.x + r.width - m,
            p[1] >= r.y + r.height - m,
        ]
    }
    pub fn resize(&mut self, start: &Pin, edge: [bool; 4], delta: [f64; 2]) {
        self.resize_scaled(start, edge, delta, 1.);
    }
    pub fn resize_scaled(&mut self, start: &Pin, edge: [bool; 4], delta: [f64; 2], scale: f64) {
        let r = start.rect();
        let [w, n, e, s] = edge;
        let dw = if w {
            -delta[0]
        } else if e {
            delta[0]
        } else {
            0.
        };
        let dh = if n {
            -delta[1]
        } else if s {
            delta[1]
        } else {
            0.
        };
        let desired = start.zoom
            + if (w || e) && (n || s) {
                (start.width * dw + start.height * dh)
                    / (start.width.powi(2) + start.height.powi(2))
            } else if w || e {
                dw / start.width
            } else {
                dh / start.height
            };
        let min = start.minimum_zoom();
        self.zoom = desired.clamp(min, self.maximum_zoom());
        let size = self.rect();
        self.x = snap_coord(
            if w {
                r.x + r.width - size.width
            } else if e {
                r.x
            } else {
                r.x + (r.width - size.width) / 2.
            },
            scale,
        );
        self.y = snap_coord(
            if n {
                r.y + r.height - size.height
            } else if s {
                r.y
            } else {
                r.y + (r.height - size.height) / 2.
            },
            scale,
        );
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Board {
    pub version: u32,
    pub group: String,
    pub groups: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        rename = "groupMeta"
    )]
    pub group_meta: BTreeMap<String, GroupMeta>,
    pub pins: Vec<Pin>,
    #[serde(skip)]
    pub previous: String,
    #[serde(skip)]
    pub hidden: bool,
    #[serde(skip)]
    pub selected: Option<u64>,
    #[serde(skip)]
    next: u64,
}
impl Default for Board {
    fn default() -> Self {
        Self {
            version: 1,
            group: DEFAULT_GROUP.into(),
            groups: vec![DEFAULT_GROUP.into()],
            group_meta: BTreeMap::new(),
            pins: vec![],
            previous: DEFAULT_GROUP.into(),
            hidden: false,
            selected: None,
            next: 1,
        }
    }
}
impl Board {
    pub fn restore(&mut self) -> Result<(), String> {
        if self.version != 1
            || self.pins.len() > STORED_LIMIT
            || self.groups.len() > MAX_GROUPS
            || self.group_meta.len() > MAX_GROUPS
            || self
                .groups
                .iter()
                .any(|s| s.trim().is_empty() || s.chars().count() > MAX_GROUP_NAME_LEN)
            || self.group_meta.iter().any(|(k, meta)| {
                k.trim().is_empty()
                    || k.chars().count() > MAX_GROUP_NAME_LEN
                    || normalize_group_color(meta.color.as_deref()).is_err()
            })
            || self.pins.iter().any(|p| !p.valid())
            || self.pins.iter().filter(|p| !p.closed).count() > ACTIVE_LIMIT
        {
            return Err(t("Invalid saved pin data"));
        }
        let mut deduped = Vec::with_capacity(self.groups.len().max(1));
        for g in std::mem::take(&mut self.groups) {
            let trimmed = g.trim().to_owned();
            if !deduped.contains(&trimmed) {
                deduped.push(trimmed);
            }
        }
        if !deduped.iter().any(|g| g == DEFAULT_GROUP) {
            deduped.insert(0, DEFAULT_GROUP.into());
        }
        self.groups = deduped;
        for (i, p) in self.pins.iter_mut().enumerate() {
            p.id = i as u64 + 1;
            let trimmed = p.group.trim();
            p.group = if trimmed.is_empty() {
                DEFAULT_GROUP.into()
            } else {
                trimmed.to_owned()
            };
            if !self.groups.contains(&p.group) {
                if self.groups.len() >= MAX_GROUPS {
                    return Err(t("Invalid saved pin data"));
                }
                self.groups.push(p.group.clone());
            }
        }
        if !self.groups.contains(&self.group) {
            self.group = DEFAULT_GROUP.into();
        }
        let mut clean_meta = BTreeMap::new();
        for (k, meta) in std::mem::take(&mut self.group_meta) {
            let key = k.trim().to_owned();
            if self.groups.contains(&key)
                && let Ok(Some(color)) = normalize_group_color(meta.color.as_deref())
            {
                clean_meta.insert(key, GroupMeta { color: Some(color) });
            }
        }
        self.group_meta = clean_meta;
        self.next = self.pins.len() as u64 + 1;
        self.previous = self.group.clone();
        Ok(())
    }
    pub fn add(&mut self, rect: Bounds) -> Result<u64, String> {
        if self.pins.iter().filter(|p| !p.closed).count() >= ACTIVE_LIMIT {
            return Err(t("At most 32 pins are allowed"));
        }
        if self.pins.len() >= STORED_LIMIT {
            return Err(t(
                "At most 128 stored pins are allowed. Destroy closed pins to free space.",
            ));
        }
        let id = self.next;
        self.next += 1;
        self.pins.push(Pin {
            id,
            image: String::new(),
            source: None,
            plain_text: None,
            grayscale: false,
            inverted: false,
            shadow: true,
            smooth: true,
            max_size: maximum_size(),
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            zoom: 1.,
            opacity: 1.,
            group: self.group.clone(),
            hidden: false,
            through: false,
            closed: false,
            closed_at: 0,
            thumbnail_zoom: None,
        });
        self.selected = Some(id);
        Ok(id)
    }
    pub fn get(&self, id: u64) -> Option<&Pin> {
        self.pins.iter().find(|p| p.id == id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Pin> {
        self.pins.iter_mut().find(|p| p.id == id)
    }
    pub fn visible(&self, p: &Pin) -> bool {
        !self.hidden && !p.closed && !p.hidden && p.group == self.group
    }
    pub fn hit(&self, point: [f64; 2]) -> Option<u64> {
        self.pins
            .iter()
            .rev()
            .find(|p| self.visible(p) && !p.through && contains(p.rect(), point))
            .map(|p| p.id)
    }
    pub fn close(&mut self, id: u64, time: u64) {
        if let Some(p) = self.get_mut(id) {
            p.closed = true;
            p.closed_at = time;
        }
        if self.selected == Some(id) {
            self.selected = None;
        }
    }
    pub fn obsolete_closed(&self, limit: usize) -> Vec<u64> {
        let mut closed: Vec<_> = self.pins.iter().filter(|p| p.closed).collect();
        closed.sort_by_key(|p| (std::cmp::Reverse(p.closed_at), std::cmp::Reverse(p.id)));
        closed.into_iter().skip(limit).map(|p| p.id).collect()
    }
    pub fn destroy(&mut self, id: u64) {
        self.pins.retain(|p| p.id != id);
        if self.selected == Some(id) {
            self.selected = None;
        }
    }
    pub fn reopen(&mut self, all: bool) -> usize {
        let capacity = ACTIVE_LIMIT - self.pins.iter().filter(|p| !p.closed).count();
        let mut ids: Vec<_> = self
            .pins
            .iter()
            .filter(|p| p.closed)
            .map(|p| (p.closed_at, p.id))
            .collect();
        ids.sort_by(|a, b| b.cmp(a));
        let mut restored = 0;
        for (_, id) in ids
            .into_iter()
            .take(if all { capacity } else { capacity.min(1) })
        {
            restored += usize::from(self.reopen_id(id));
        }
        restored
    }
    pub fn reopen_id(&mut self, id: u64) -> bool {
        if self.pins.iter().filter(|p| !p.closed).count() >= ACTIVE_LIMIT {
            return false;
        }
        let Some(p) = self.get_mut(id).filter(|p| p.closed) else {
            return false;
        };
        p.closed = false;
        p.hidden = false;
        let group = p.group.clone();
        self.group = group;
        self.hidden = false;
        self.selected = Some(id);
        true
    }
    pub fn switch_group(&mut self, name: Option<&str>) -> Result<(), String> {
        let next = if let Some(name) = name.map(str::trim).filter(|s| !s.is_empty()) {
            if name.eq_ignore_ascii_case(DEFAULT_GROUP)
                && self.groups.iter().any(|g| g == DEFAULT_GROUP)
            {
                DEFAULT_GROUP.to_owned()
            } else {
                name.to_owned()
            }
        } else {
            let index = self
                .groups
                .iter()
                .position(|g| g == &self.group)
                .unwrap_or(0);
            self.groups[(index + 1) % self.groups.len()].clone()
        };
        if !self.groups.contains(&next) {
            return Err(tf("Pin group not found: {0}", &[&next]));
        }
        if self.group != next {
            self.previous = self.group.clone();
            self.group = next;
        }
        self.hidden = false;
        Ok(())
    }
    pub fn create_group(&mut self, name: String) -> Result<(), String> {
        self.create_group_with(&name, None, true, false).map(|_| ())
    }
    pub fn create_group_with(
        &mut self,
        name: &str,
        color: Option<&str>,
        activate: bool,
        strict_unique: bool,
    ) -> Result<String, String> {
        let trimmed = validate_group_name(name)?;
        let normalized_color = normalize_group_color(color)?;
        let canonical = if !strict_unique && trimmed.eq_ignore_ascii_case(DEFAULT_GROUP) {
            DEFAULT_GROUP.to_owned()
        } else {
            trimmed
        };
        let collides_default = canonical.eq_ignore_ascii_case(DEFAULT_GROUP)
            || canonical == group_display_name(DEFAULT_GROUP);
        if strict_unique && (collides_default || self.groups.contains(&canonical)) {
            return Err(t("This pin group already exists"));
        }
        if !self.groups.contains(&canonical) {
            if self.groups.len() >= MAX_GROUPS {
                return Err(t("The group name or group count exceeds its limit"));
            }
            self.groups.push(canonical.clone());
        }
        if let Some(hex) = normalized_color {
            self.group_meta
                .insert(canonical.clone(), GroupMeta { color: Some(hex) });
        }
        if activate {
            self.switch_group(Some(&canonical))?;
        }
        Ok(canonical)
    }
    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<String, String> {
        let old = old.trim();
        if old == DEFAULT_GROUP {
            return Err(t("The default pin group cannot be renamed"));
        }
        let Some(idx) = self.groups.iter().position(|g| g == old) else {
            return Err(tf("Pin group not found: {0}", &[old]));
        };
        let next = validate_group_name(new)?;
        if next == old {
            return Ok(next);
        }
        if next.eq_ignore_ascii_case(DEFAULT_GROUP)
            || next == group_display_name(DEFAULT_GROUP)
            || self.groups.contains(&next)
        {
            return Err(t("This pin group already exists"));
        }
        self.groups[idx] = next.clone();
        if let Some(meta) = self.group_meta.remove(old) {
            self.group_meta.insert(next.clone(), meta);
        }
        if self.group == old {
            self.group = next.clone();
        }
        if self.previous == old {
            self.previous = next.clone();
        }
        for pin in &mut self.pins {
            if pin.group == old {
                pin.group = next.clone();
            }
        }
        Ok(next)
    }
    pub fn set_group_color(&mut self, name: &str, color: Option<&str>) -> Result<(), String> {
        let key = if name.trim().eq_ignore_ascii_case(DEFAULT_GROUP) {
            DEFAULT_GROUP
        } else {
            name.trim()
        };
        if !self.groups.iter().any(|g| g == key) {
            return Err(tf("Pin group not found: {0}", &[key]));
        }
        if let Some(hex) = normalize_group_color(color)? {
            self.group_meta
                .insert(key.to_owned(), GroupMeta { color: Some(hex) });
        } else {
            self.group_meta.remove(key);
        }
        Ok(())
    }
    pub fn group_color(&self, name: &str) -> Option<&str> {
        self.group_meta
            .get(name)
            .and_then(|meta| meta.color.as_deref())
    }
    pub fn reorder_groups(&mut self, order: Vec<String>) -> Result<(), String> {
        if order.len() != self.groups.len() || !self.groups.iter().all(|g| order.contains(g)) {
            return Err(t("Invalid pin group order"));
        }
        self.groups = order;
        Ok(())
    }
    pub fn move_pins_to_group(&mut self, ids: &[u64], target: &str) -> Result<usize, String> {
        let dest = if target.trim().eq_ignore_ascii_case(DEFAULT_GROUP)
            && self.groups.iter().any(|g| g == DEFAULT_GROUP)
        {
            DEFAULT_GROUP
        } else {
            target.trim()
        };
        if !self.groups.iter().any(|g| g == dest) {
            return Err(tf("Pin group not found: {0}", &[dest]));
        }
        let mut moved = 0;
        for pin in &mut self.pins {
            if ids.contains(&pin.id) && pin.group != dest {
                pin.group = dest.to_owned();
                pin.hidden = false;
                moved += 1;
            }
        }
        if let Some(selected) = self.selected
            && self
                .pins
                .iter()
                .find(|p| p.id == selected)
                .is_none_or(|p| p.group != self.group || p.closed)
        {
            self.selected = None;
        }
        Ok(moved)
    }
    pub fn remove_group(
        &mut self,
        name: &str,
        migrate_to: Option<&str>,
    ) -> Result<(usize, Vec<u64>), String> {
        let target = name.trim();
        if target == DEFAULT_GROUP {
            return Err(t("The default pin group cannot be deleted"));
        }
        if !self.groups.iter().any(|g| g == target) {
            return Err(tf("Pin group not found: {0}", &[target]));
        }
        let destination = migrate_to
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.eq_ignore_ascii_case(DEFAULT_GROUP)
                    && self.groups.iter().any(|g| g == DEFAULT_GROUP)
                {
                    DEFAULT_GROUP
                } else {
                    s
                }
            });
        let (migrated, destroyed) = if let Some(dest) = destination {
            if dest == target {
                return Err(t("Choose a different target pin group"));
            }
            if !self.groups.iter().any(|g| g == dest) {
                return Err(tf("Pin group not found: {0}", &[dest]));
            }
            let mut count = 0;
            for pin in &mut self.pins {
                if pin.group == target {
                    pin.group = dest.to_owned();
                    count += 1;
                }
            }
            (count, vec![])
        } else {
            let ids: Vec<u64> = self
                .pins
                .iter()
                .filter(|p| p.group == target)
                .map(|p| p.id)
                .collect();
            self.pins.retain(|p| p.group != target);
            if self.selected.is_some_and(|id| ids.contains(&id)) {
                self.selected = None;
            }
            (0, ids)
        };
        self.groups.retain(|g| g != target);
        self.group_meta.remove(target);
        if self.groups.is_empty() {
            self.groups.push(DEFAULT_GROUP.into());
        }
        if self.group == target {
            self.group = destination
                .filter(|d| self.groups.iter().any(|g| g == *d))
                .map(str::to_owned)
                .or_else(|| {
                    (self.previous != target && self.groups.contains(&self.previous))
                        .then(|| self.previous.clone())
                })
                .unwrap_or_else(|| DEFAULT_GROUP.into());
            self.hidden = false;
        }
        if self.previous == target || !self.groups.contains(&self.previous) {
            self.previous = self.group.clone();
        }
        Ok((migrated, destroyed))
    }
    pub fn has_empty_groups(&self) -> bool {
        self.groups.iter().any(|g| {
            g != DEFAULT_GROUP && g != &self.group && !self.pins.iter().any(|p| &p.group == g)
        })
    }
    pub fn remove_empty_groups(&mut self) -> Vec<String> {
        let mut removed = Vec::new();
        for g in &self.groups {
            if g != DEFAULT_GROUP && g != &self.group && !self.pins.iter().any(|p| &p.group == g) {
                removed.push(g.clone());
            }
        }
        if !removed.is_empty() {
            self.groups.retain(|g| !removed.contains(g));
            for g in &removed {
                self.group_meta.remove(g);
            }
            if !self.groups.contains(&self.previous) {
                self.previous = self.group.clone();
            }
        }
        removed
    }
    pub fn group_summaries(&self) -> Vec<GroupSummary> {
        self.groups
            .iter()
            .map(|name| {
                let mut active_pins = 0;
                let mut closed_pins = 0;
                for pin in &self.pins {
                    if &pin.group == name {
                        if pin.closed {
                            closed_pins += 1;
                        } else {
                            active_pins += 1;
                        }
                    }
                }
                GroupSummary {
                    name: name.clone(),
                    display_name: group_display_name(name),
                    color: self.group_color(name).map(str::to_owned),
                    active_pins,
                    closed_pins,
                    current: name == &self.group,
                    default: name == DEFAULT_GROUP,
                }
            })
            .collect()
    }
    pub fn keep_reachable(&mut self, outputs: &[Output]) {
        for p in &mut self.pins {
            let r = p.rect();
            let center = r.center();
            let Some(m) = outputs.iter().min_by(|a, b| {
                let distance = |r: Bounds| {
                    (center[0] - center[0].clamp(r.x, r.x + r.width)).powi(2)
                        + (center[1] - center[1].clamp(r.y, r.y + r.height)).powi(2)
                };
                distance(a.rect).total_cmp(&distance(b.rect))
            }) else {
                continue;
            };
            p.x =
                p.x.clamp(m.rect.x - r.width + 32., m.rect.x + m.rect.width - 32.);
            p.y = p.y.clamp(m.rect.y, m.rect.y + m.rect.height - 32.);
        }
    }
}
/// The extra transparent gutter prevents Mutter classifying an OR pin host as
/// fullscreen. It changes the host only, never the image's size or origin.
pub fn host_rect(monitor: Bounds, outputs: &[Output]) -> Bounds {
    let limit = crate::capture::Selection::new(outputs.to_vec(), vec![], None)
        .map(|s| s.limit)
        .unwrap_or(monitor);
    let mut gutter = 1.;
    while outputs
        .iter()
        .map(|o| o.rect)
        .chain([Bounds {
            x: 0.,
            y: 0.,
            ..limit
        }])
        .any(|r| {
            r.x == monitor.x
                && r.y == monitor.y
                && r.width == monitor.width + gutter
                && r.height == monitor.height + gutter
        })
    {
        gutter += 1.;
    }
    Bounds {
        width: monitor.width + gutter,
        height: monitor.height + gutter,
        ..monitor
    }
}
pub fn cursor(edge: [bool; 4]) -> &'static str {
    match edge {
        [true, true, _, _] | [false, false, true, true] => "nwse-resize",
        [true, false, false, true] | [false, true, true, false] => "nesw-resize",
        [true, false, false, false] | [false, false, true, false] => "ew-resize",
        [false, true, false, false] | [false, false, false, true] => "ns-resize",
        _ => "default",
    }
}
pub fn overlaps(a: Bounds, b: Bounds) -> bool {
    let r = intersect(a, b);
    r.width > 0. && r.height > 0.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_size_keeps_small_and_fractional_logical_dimensions() {
        let mut board = Board::default();
        for (width, height) in [(150.5, 90.5), (8., 8.), (0.5, 0.5)] {
            let bounds = Bounds {
                x: 0.,
                y: 0.,
                width,
                height,
            };
            let id = board.add(bounds).unwrap();
            let pin = board.get(id).unwrap();
            assert_eq!(pin.rect(), bounds);
            assert!(pin.minimum_zoom() <= 1.);
        }
    }
    #[test]
    fn closing_keeps_original_geometry_and_reopening_is_recent_first() {
        let mut b = Board::default();
        let r = Bounds {
            x: -100.,
            y: 80.,
            width: 2880.,
            height: 1800.,
        };
        let a = b.add(r).unwrap();
        let c = b.add(r).unwrap();
        b.close(a, 1);
        b.close(c, 2);
        assert_eq!(b.reopen(false), 1);
        assert!(b.get(a).unwrap().closed);
        assert_eq!(b.get(c).unwrap().rect(), r);
        assert_eq!(b.reopen(true), 1);
        b.destroy(a);
        assert!(b.get(a).is_none());
    }
    #[test]
    fn resize_preserves_opposite_edges_and_thin_image_aspect() {
        let mut b = Board::default();
        let id = b
            .add(Bounds {
                x: 20.,
                y: 30.,
                width: 500.,
                height: 10.,
            })
            .unwrap();
        let start = b.get(id).unwrap().clone();
        let p = b.get_mut(id).unwrap();
        p.resize(&start, [true, false, false, false], [100., 0.]);
        let r = p.rect();
        assert_eq!(r.x + r.width, 520.);
        assert_eq!(r.width, 400.);
        assert_eq!(r.height, 8.);

        assert_eq!(snap_coord(10.5, 2.), 10.5);
        assert_eq!(snap_coord(10.24, 2.), 10.);
        assert_eq!(snap_coord(10.26, 2.), 10.5);
        let mut p2 = start.clone();
        p2.x = 20.5;
        let anchor = p2.clone();
        p2.resize_scaled(&anchor, [true, false, false, false], [100., 0.], 2.);
        assert_eq!(p2.rect().x + p2.rect().width, 520.5);
    }
    #[test]
    fn fullscreen_image_keeps_pixels_but_host_has_gutter() {
        let r = Bounds {
            x: 0.,
            y: 0.,
            width: 1280.,
            height: 800.,
        };
        let outputs = [Output { rect: r, scale: 1. }];
        assert_eq!(host_rect(r, &outputs).width, 1281.);
        let mut b = Board::default();
        let id = b.add(r).unwrap();
        assert_eq!(b.get(id).unwrap().rect(), r);
    }
    #[test]
    fn group_create_rename_color_reorder_move_migrate_delete_and_cleanup() {
        let rect = Bounds {
            x: 10.,
            y: 20.,
            width: 200.,
            height: 120.,
        };
        let mut b = Board::default();
        let p1 = b.add(rect).unwrap();
        let p2 = b.add(rect).unwrap();
        assert!(b.create_group_with("   ", None, true, true).is_err());
        assert!(b.create_group_with("default", None, true, true).is_err());

        let g_work = b
            .create_group_with(" Work ", Some("#FF5555"), false, true)
            .unwrap();
        assert_eq!(g_work, "Work");
        assert_eq!(b.group, "default");
        assert_eq!(b.group_color("Work"), Some("#ff5555"));
        assert!(b.create_group_with("Work", None, true, true).is_err());

        b.create_group_with("EmptyOne", None, false, true).unwrap();
        b.create_group_with("Archive", Some("#50fa7b"), true, true)
            .unwrap();
        assert_eq!(b.group, "Archive");
        assert_eq!(b.previous, "default");

        assert_eq!(b.move_pins_to_group(&[p1, p2], "Work").unwrap(), 2);
        assert_eq!(b.get(p1).unwrap().group, "Work");
        assert_eq!(b.get(p2).unwrap().group, "Work");

        assert!(b.rename_group("default", "Custom").is_err());
        let renamed = b.rename_group("Work", "Design").unwrap();
        assert_eq!(renamed, "Design");
        assert_eq!(b.group_color("Design"), Some("#ff5555"));
        assert_eq!(b.get(p1).unwrap().group, "Design");

        b.reorder_groups(vec![
            "default".into(),
            "Archive".into(),
            "Design".into(),
            "EmptyOne".into(),
        ])
        .unwrap();
        assert_eq!(b.groups, vec!["default", "Archive", "Design", "EmptyOne"]);

        assert!(b.has_empty_groups());
        // Archive is currently active so remove_empty_groups keeps it, removing EmptyOne only.
        assert_eq!(b.remove_empty_groups(), vec!["EmptyOne".to_owned()]);
        assert!(!b.has_empty_groups());

        assert!(b.remove_group("default", None).is_err());
        // Migrate Design's 2 pins into Archive on deletion.
        let (migrated, destroyed) = b.remove_group("Design", Some("Archive")).unwrap();
        assert_eq!(migrated, 2);
        assert!(destroyed.is_empty());
        assert_eq!(b.get(p1).unwrap().group, "Archive");

        // Deleting active Archive without migrate_to destroys its pins and falls back cleanly.
        let (migrated2, destroyed2) = b.remove_group("Archive", None).unwrap();
        assert_eq!(migrated2, 0);
        assert_eq!(destroyed2, vec![p1, p2]);
        assert!(b.pins.is_empty());
        assert_eq!(b.group, "default");
    }
    #[test]
    fn legacy_v1_json_without_group_meta_restores_and_roundtrips() {
        let raw = r##"{"version":1,"group":"review","groups":["default","review"],"pins":[]}"##;
        let mut board: Board = serde_json::from_str(raw).unwrap();
        board.restore().unwrap();
        assert_eq!(board.group, "review");
        assert!(board.group_meta.is_empty());
        let saved = serde_json::to_string(&board).unwrap();
        assert!(!saved.contains("groupMeta"));

        board.set_group_color("review", Some("#BD93F9")).unwrap();
        let with_meta = serde_json::to_string(&board).unwrap();
        assert!(with_meta.contains("\"groupMeta\""));
        let mut reloaded: Board = serde_json::from_str(&with_meta).unwrap();
        reloaded.restore().unwrap();
        assert_eq!(reloaded.group_color("review"), Some("#bd93f9"));
    }
}
