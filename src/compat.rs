//! How a game runs on Apple Silicon, from highball-db (gauthierpiarrette/highball-db):
//! curated entries (CC0-1.0) and its anti-cheat map, derived from Are We Anti-Cheat Yet?
//! (MIT). The ProtonDB-based predictions (ODbL) are not used.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const GAMES: &str = "https://gethighball.com/data/games.json";
pub const ANTICHEAT: &str = "https://raw.githubusercontent.com/gauthierpiarrette/highball-db/main/db/anticheat.json";

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Compat {
    pub title: String,
    /// verified-local, reported-upstream, community, blocked-anticheat, blocked-publisher,
    /// or anticheat-warning for a game only the anti-cheat map knows.
    pub status: String,
    /// The renderer Highball uses: dxmt, d3dmetal, dxvk, wined3d, vkd3d.
    pub renderer: Option<String>,
    pub notes: String,
    pub anticheat: Vec<String>,
    pub url: Option<String>,
}

impl Compat {
    /// "bad" (does not run on a Mac), "warn", "good" or "info".
    pub fn level(&self) -> &'static str {
        match self.status.as_str() {
            s if s.starts_with("blocked") => "bad",
            "anticheat-warning" => "warn",
            "verified-local" => "good",
            _ if !self.anticheat.is_empty() => "warn",
            _ => "info",
        }
    }

    pub fn label(&self) -> &'static str {
        match self.status.as_str() {
            "verified-local" => "Comprobado en un Mac con Apple Silicon",
            "reported-upstream" => "Funciona según las notas de sus desarrolladores",
            "community" => "Funciona según la comunidad",
            "blocked-anticheat" => "No funciona en Mac: antitrampas del sistema",
            "blocked-publisher" => "No funciona en Mac: su editora lo bloquea",
            "anticheat-warning" => "Usa antitrampas: puede no funcionar",
            _ => "Sin datos",
        }
    }

    /// What Highball's renderer means on the D3DMetal engine, whose Auto follows it.
    pub fn renderer_note_d3dmetal(&self) -> Option<String> {
        Some(match self.renderer.as_deref()? {
            "d3dmetal" | "vkd3d" => "Recomendado: D3DMetal, el que usa Auto".into(),
            "dxmt" => "Recomendado: DXMT, el que usa Auto".into(),
            "wined3d" => "Recomendado: WineD3D (Auto lo usa)".into(),
            "dxvk" => "En Highball va con DXVK, que este motor no tiene; si falla, prueba WineD3D".into(),
            _ => return None,
        })
    }

    /// What Highball's renderer means on this engine, when it differs from Auto.
    pub fn renderer_note(&self) -> Option<String> {
        Some(match self.renderer.as_deref()? {
            "wined3d" => "Recomendado: WineD3D (Auto lo usa)".into(),
            "dxmt" => "Recomendado: DXMT, el que usa Auto".into(),
            "vkd3d" => "Recomendado: vkd3d-proton, el que usa Auto".into(),
            "d3dmetal" => "En Highball va con D3DMetal; aquí va con DXMT o vkd3d-proton".into(),
            "dxvk" => "En Highball va con DXVK, que One-Side no trae; si falla, prueba WineD3D".into(),
            _ => return None,
        })
    }
}

/// Lower-case letters and digits only, so "DOOM Eternal™" and "Doom: Eternal" meet.
pub fn norm(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

fn appid(v: &Value) -> Option<String> {
    v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
}

/// Builds the map from games.json and anticheat.json. Keys: "steam/<app id>" and
/// "title/<normalized title>".
pub fn parse(games: &Value, anticheat: &Value) -> HashMap<String, Compat> {
    let mut out = HashMap::new();
    let put = |out: &mut HashMap<String, Compat>, id: Option<String>, c: Compat| {
        if let Some(id) = id {
            out.entry(format!("steam/{id}")).or_insert_with(|| c.clone());
        }
        let t = norm(&c.title);
        if !t.is_empty() {
            out.entry(format!("title/{t}")).or_insert(c);
        }
    };
    for g in games["games"].as_array().into_iter().flatten() {
        let c = Compat {
            title: s(&g["title"]),
            status: s(&g["status"]),
            renderer: g["renderer"].as_str().map(str::to_owned),
            notes: s(&g["notes"]),
            anticheat: g["anticheat"]["names"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_owned)).collect(),
            url: g["url"].as_str().map(str::to_owned),
        };
        if !c.title.is_empty() && !c.status.is_empty() {
            put(&mut out, appid(&g["steam_appid"]), c);
        }
    }
    for g in anticheat["games"].as_object().into_iter().flat_map(|m| m.values()) {
        let blocked = g["macVerdict"].as_str() == Some("blocked");
        let names: Vec<String> = g["anticheats"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_owned)).collect();
        let c = Compat {
            title: s(&g["title"]),
            status: if blocked { "blocked-anticheat" } else { "anticheat-warning" }.into(),
            renderer: None,
            notes: if blocked {
                format!("Antitrampas: {}. Funciona dentro del sistema de Windows, y eso no existe en Wine.", names.join(", "))
            } else {
                format!("Antitrampas: {}. Funciona en Linux, pero en Mac no se ha comprobado: el modo en línea puede fallar.", names.join(", "))
            },
            anticheat: names,
            url: None,
        };
        if !c.title.is_empty() {
            put(&mut out, appid(&g["steam_appid"]), c);
        }
    }
    out
}

pub fn find<'a>(map: &'a HashMap<String, Compat>, store: &str, id: &str, name: &str) -> Option<&'a Compat> {
    (store == "steam").then(|| map.get(&format!("steam/{id}"))).flatten().or_else(|| map.get(&format!("title/{}", norm(name))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_finds() {
        let games = json!({ "games": [
            { "title": "Aperture Desk Job", "status": "verified-local", "steam_appid": 1902490, "renderer": "dxvk", "notes": "ok", "anticheat": null, "url": "https://gethighball.com/games/aperture-desk-job/" },
            { "title": "2XKO", "status": "blocked-anticheat", "steam_appid": null, "renderer": null, "notes": "", "anticheat": { "names": ["Vanguard"] }, "url": null },
        ]});
        let ac = json!({ "games": {
            "halo": { "title": "Halo: The Master Chief Collection", "anticheats": ["Easy Anti-Cheat"], "steam_appid": 976730, "macVerdict": "warning" },
            "dup": { "title": "Aperture Desk Job", "anticheats": ["X"], "steam_appid": 1902490, "macVerdict": "blocked" },
        }});
        let m = parse(&games, &ac);
        let a = find(&m, "steam", "1902490", "whatever").unwrap();
        assert_eq!((a.level(), a.renderer.as_deref()), ("good", Some("dxvk")), "curated entries win over the anti-cheat map");
        assert_eq!(find(&m, "egs", "x", "2XKO™").unwrap().level(), "bad");
        let h = find(&m, "custom", "", "Halo - The Master Chief Collection").unwrap();
        assert_eq!((h.level(), h.anticheat.clone()), ("warn", vec!["Easy Anti-Cheat".to_owned()]));
        assert!(find(&m, "steam", "1", "Nothing").is_none());
    }
}
