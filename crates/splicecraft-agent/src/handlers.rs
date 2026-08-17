//! First-wave endpoints for stages 01–13. Same registry HTTP and CLI use.

use serde_json::{Value, json};

use splicecraft_bio::{
    BlastProgram, ORF_DEFAULT_MIN_AA, ScanOptions, blast_search, detect_query_program, find_orfs,
    scan_restriction_sites,
};
use splicecraft_gels::{GelStore, PCR_DEFAULT_MAX_AMPLICON, simulate_pcr};
use splicecraft_io::{
    CancellationToken, OfflineTransport, OnlineSearchPolicy, blast_db_from_library,
    export_genbank_to_path, gb_text_to_record, hmmer_web_hmmscan, load_path, ncbi_blast_online,
    record_to_library_entry,
};
use splicecraft_persist::{
    self as persist, CollisionChoice, ExperimentStore, KeepOutcome, LibraryEntry,
    SETTING_ALLOW_ONLINE_LOOKUPS, SETTING_ALLOW_ONLINE_SEARCH, allow_online_search,
    load_hmm_catalog, load_primers, load_settings_map, set_setting_bool,
};
use splicecraft_util::sanitize_label;

use crate::dispatch::{AgentResponse, persist_save_error, require_persist_writes};
use crate::paths::{check_read_path, check_write_path, sanitize_agent_path, scrub_path};
use crate::registry::{Endpoint, EndpointMethod, Registry};
use crate::session::AgentSession;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const DANGEROUS: &[&str] = &[
    "collection",
    "source_collection",
    "bin",
    "parts_bin",
    "enzyme",
    "enzymes",
    "orientation",
    "rename",
];

/// Fill the builtin registry.
#[must_use]
pub fn register_builtin() -> Registry {
    let mut r = Registry::new();
    reg(
        &mut r,
        "healthz",
        EndpointMethod::Get,
        false,
        "Readiness probe (liveness, version, headless).",
        "No body. Unauthenticated over HTTP.",
        json!({"type": "object", "properties": {}}),
        healthz,
    );
    reg(
        &mut r,
        "tools",
        EndpointMethod::Get,
        false,
        "Self-describe every endpoint with schema and docs.",
        "No body. Unauthenticated over HTTP. Returns {endpoints: [...]} with schema.",
        json!({"type": "object", "properties": {}}),
        tools_placeholder,
    );
    reg(
        &mut r,
        "status",
        EndpointMethod::Get,
        false,
        "Loaded record snapshot (name, length, circular, dirty). No sequence.",
        "No body. Sequence is never returned.",
        json!({"type": "object", "properties": {}}),
        status,
    );
    reg(
        &mut r,
        "list-library",
        EndpointMethod::Get,
        false,
        "Plasmid library entries (name, id, length, n_features, topology, source).",
        "Body: {collection?}. Omit collection for the full library.",
        json!({
            "type": "object",
            "properties": {"collection": {"type": "string"}}
        }),
        list_library,
    );
    reg(
        &mut r,
        "list-collections",
        EndpointMethod::Get,
        false,
        "Collection buckets with plasmid counts and the active name.",
        "No body.",
        json!({"type": "object", "properties": {}}),
        list_collections,
    );
    reg(
        &mut r,
        "search-library",
        EndpointMethod::Get,
        false,
        "Fuzzy plasmid-name search across collections.",
        "Body: {query?: str, limit?: int}. Empty query returns the first limit rows.",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
            }
        }),
        search_library,
    );
    reg(
        &mut r,
        "load-entry",
        EndpointMethod::Post,
        true,
        "Load a library entry onto the canvas by name or id.",
        "Body: {name|id, collection?, force?}. 404 if missing, 409 if ambiguous.",
        json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "id": {"type": "string"},
                "collection": {"type": "string"},
                "force": {"type": "boolean"}
            }
        }),
        load_entry,
    );
    reg(
        &mut r,
        "features",
        EndpointMethod::Get,
        false,
        "Features on the loaded record.",
        "No body. 422 if nothing is loaded.",
        json!({"type": "object", "properties": {}}),
        list_features,
    );
    reg(
        &mut r,
        "list-features",
        EndpointMethod::Get,
        false,
        "Alias of features.",
        "No body. 422 if nothing is loaded.",
        json!({"type": "object", "properties": {}}),
        list_features,
    );
    reg(
        &mut r,
        "get-sequence",
        EndpointMethod::Get,
        false,
        "Loaded sequence (JSON only; never logged).",
        "No body. 422 if nothing is loaded.",
        json!({"type": "object", "properties": {}}),
        get_sequence,
    );
    reg(
        &mut r,
        "find-orfs",
        EndpointMethod::Get,
        false,
        "Six-frame ORF scan. Filter by min_aa (amino acids), never min_length/min_bp.",
        "Body: {min_aa?: int, include_alt_starts?: bool}. Returns length_aa, nt_len, exceeds_one_lap.",
        json!({
            "type": "object",
            "properties": {
                "min_aa": {"type": "integer", "minimum": 1, "default": 30},
                "include_alt_starts": {"type": "boolean"}
            }
        }),
        find_orfs_handler,
    );
    reg(
        &mut r,
        "list-restriction-sites",
        EndpointMethod::Get,
        false,
        "Restriction-site scan of the loaded record.",
        "Body: {enzymes?: [str], enzyme?: str, min_length?: int, unique_only?: bool}.",
        json!({
            "type": "object",
            "properties": {
                "enzymes": {"type": "array", "items": {"type": "string"}},
                "enzyme": {"type": "string"},
                "min_length": {"type": "integer", "minimum": 1},
                "unique_only": {"type": "boolean"}
            }
        }),
        list_restriction_sites,
    );
    reg(
        &mut r,
        "save",
        EndpointMethod::Post,
        true,
        "Persist the loaded record into the library (create:true for homeless records).",
        "Body: {create?: bool, force?}. Disk writes use safe_save_json.",
        json!({
            "type": "object",
            "properties": {
                "create": {"type": "boolean"},
                "force": {"type": "boolean"}
            }
        }),
        save,
    );
    reg(
        &mut r,
        "add-current-to-library",
        EndpointMethod::Post,
        true,
        "Keep the loaded record in the active collection.",
        "Body: {id?, name?, choice?: skip|copy|overwrite, force?}.",
        json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"},
                "choice": {"type": "string", "enum": ["skip", "copy", "overwrite"]},
                "force": {"type": "boolean"}
            }
        }),
        add_current_to_library,
    );
    reg(
        &mut r,
        "delete-from-library",
        EndpointMethod::Post,
        true,
        "Delete one library entry by name from the active collection. Not a master wipe.",
        "Body: {name, force?}. 404 if missing.",
        json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string"},
                "force": {"type": "boolean"}
            }
        }),
        delete_from_library,
    );
    reg(
        &mut r,
        "get-settings",
        EndpointMethod::Get,
        false,
        "Persisted settings map (allow_online_search defaults false).",
        "No body.",
        json!({"type": "object", "properties": {}}),
        get_settings,
    );
    reg(
        &mut r,
        "set-setting",
        EndpointMethod::Post,
        true,
        "Set a boolean setting. Cannot enable online search/lookups.",
        "Body: {key, value}. allow_online_search / allow_online_lookups are refused.",
        json!({
            "type": "object",
            "required": ["key", "value"],
            "properties": {
                "key": {"type": "string"},
                "value": {"type": "boolean"},
                "force": {"type": "boolean"}
            }
        }),
        set_setting,
    );
    reg(
        &mut r,
        "list-hmm-databases",
        EndpointMethod::Get,
        false,
        "HMM-DB catalog (builtins re-injected). Does not download Pfam.",
        "No body.",
        json!({"type": "object", "properties": {}}),
        list_hmm_databases,
    );
    reg(
        &mut r,
        "blast",
        EndpointMethod::Get,
        false,
        "In-process ungapped BLAST against the library. Never leaves the box.",
        "Body: {query, program?: blastn|blastp|hmmscan, max_hits?: int}.",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "program": {"type": "string"},
                "max_hits": {"type": "integer"}
            }
        }),
        blast_local,
    );
    reg(
        &mut r,
        "blast-online",
        EndpointMethod::Get,
        false,
        "Remote NCBI BLAST. Refused unless allow_online_search is on.",
        "Body: {query, program?, database?, max_hits?}. 403 when the setting is off.",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "program": {"type": "string"},
                "database": {"type": "string"},
                "max_hits": {"type": "integer"}
            }
        }),
        blast_online,
    );
    reg(
        &mut r,
        "hmmscan-online",
        EndpointMethod::Get,
        false,
        "Remote EBI HMMER hmmscan. Refused unless allow_online_search is on.",
        "Body: {query, max_hits?}. Alias of hmmer-web.",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "max_hits": {"type": "integer"}
            }
        }),
        hmmscan_online,
    );
    reg(
        &mut r,
        "hmmer-web",
        EndpointMethod::Get,
        false,
        "Alias of hmmscan-online.",
        "Body: {query, max_hits?}.",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "max_hits": {"type": "integer"}
            }
        }),
        hmmscan_online,
    );
    reg(
        &mut r,
        "load-file",
        EndpointMethod::Post,
        true,
        "Load a local GenBank/FASTA/.dna path onto the canvas.",
        "Body: {path, force?}. ~otheruser and .. are refused; ancestor symlinks refused.",
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string"},
                "force": {"type": "boolean"}
            }
        }),
        load_file,
    );
    reg(
        &mut r,
        "export-genbank",
        EndpointMethod::Post,
        true,
        "Write the loaded record as GenBank to a sanitised path.",
        "Body: {path, force?}. Parent must exist; dest symlink refused.",
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string"},
                "force": {"type": "boolean"}
            }
        }),
        export_genbank,
    );
    reg(
        &mut r,
        "list-experiments",
        EndpointMethod::Get,
        false,
        "Lab-notebook entries (id, title, timestamps). No body text dump.",
        "No body.",
        json!({"type": "object", "properties": {}}),
        list_experiments,
    );
    reg(
        &mut r,
        "list-primers",
        EndpointMethod::Get,
        false,
        "Primer library names (sequences omitted).",
        "No body.",
        json!({"type": "object", "properties": {}}),
        list_primers,
    );
    reg(
        &mut r,
        "list-gels",
        EndpointMethod::Get,
        false,
        "Saved gel snapshots (id, name).",
        "No body.",
        json!({"type": "object", "properties": {}}),
        list_gels,
    );
    reg(
        &mut r,
        "simulate-pcr",
        EndpointMethod::Get,
        false,
        "Exact-match PCR on the loaded record (plus 3′ partial for 5′ flaps).",
        "Body: {forward, reverse, max_amplicon?}. Capped at 50 amplicons.",
        json!({
            "type": "object",
            "required": ["forward", "reverse"],
            "properties": {
                "forward": {"type": "string"},
                "reverse": {"type": "string"},
                "max_amplicon": {"type": "integer"}
            }
        }),
        simulate_pcr_handler,
    );
    r
}

#[allow(clippy::too_many_arguments)]
fn reg(
    r: &mut Registry,
    name: &'static str,
    method: EndpointMethod,
    write: bool,
    doc: &'static str,
    doc_full: &'static str,
    schema: Value,
    handler: fn(&mut AgentSession, &Value) -> AgentResponse,
) {
    r.register(Endpoint {
        name,
        method,
        write,
        doc,
        doc_full,
        schema,
        handler,
    });
}

fn tools_placeholder(_session: &mut AgentSession, _payload: &Value) -> AgentResponse {
    AgentResponse::ok(crate::registry::builtin().tools_document())
}

pub(crate) fn healthz(session: &mut AgentSession, _payload: &Value) -> AgentResponse {
    AgentResponse::ok(json!({
        "ok": true,
        "status": "ready",
        "version": VERSION,
        "headless": session.headless,
    }))
}

fn status(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let (name, id, length, circular) = match &session.record {
        Some(rec) => (
            Value::String(rec.name.clone()),
            Value::String(rec.id.clone()),
            rec.len(),
            rec.circular,
        ),
        None => (Value::Null, Value::Null, 0, false),
    };
    persist::log_event(
        "agent.status",
        &[
            ("loaded", &session.record.is_some().to_string()),
            ("length", &length.to_string()),
        ],
    );
    AgentResponse::ok(json!({
        "loaded": session.record.is_some(),
        "name": name,
        "id": id,
        "length": length,
        "circular": circular,
        "dirty": session.dirty,
        "source_path": session.source_path.as_ref().map(|p| scrub_path(&p.display().to_string())),
    }))
}

fn list_library(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &["collection"]) {
        return e;
    }
    let coll_in = payload.get("collection").and_then(Value::as_str);
    let (entries, scope) = if let Some(raw) = coll_in.filter(|s| !s.is_empty()) {
        let Some(col) = find_collection(session, raw) else {
            return AgentResponse::err(404, format!("no collection named {raw:?}"));
        };
        (col.plasmids.clone(), Some(col.name.clone()))
    } else {
        let mut all = Vec::new();
        for col in &session.library.collections {
            all.extend(col.plasmids.iter().cloned());
        }
        if all.is_empty() {
            all = session.library.plasmids.clone();
        }
        (all, None)
    };
    let library: Vec<Value> = entries.iter().map(project_entry).collect();
    let count = library.len();
    let mut body = json!({"library": library, "count": count});
    if let Some(name) = scope {
        body["collection"] = Value::String(name);
    }
    AgentResponse::ok(body)
}

fn list_collections(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let collections: Vec<Value> = session
        .library
        .collections
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "n_plasmids": c.plasmids.len(),
            })
        })
        .collect();
    AgentResponse::ok(json!({
        "active": session.library.active,
        "collections": collections,
    }))
}

fn search_library(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &["collection"]) {
        return e;
    }
    let query = payload.get("query").and_then(Value::as_str).unwrap_or("");
    let limit = match coerce_int(payload.get("limit"), "limit") {
        Ok(None) => 200,
        Ok(Some(n)) if n < 1 => {
            return AgentResponse::err(400, "'limit' must be ≥ 1");
        }
        Ok(Some(n)) => n.min(1000) as usize,
        Err(e) => return e,
    };
    let mut matches = Vec::new();
    for col in &session.library.collections {
        for e in &col.plasmids {
            if fuzzy_text_match(query, &e.name) || fuzzy_text_match(query, &e.id) {
                matches.push(json!({
                    "collection": col.name,
                    "name": e.name,
                    "id": e.id,
                    "size": e.size,
                    "n_feats": n_features(e),
                    "source": e.source,
                }));
                if matches.len() >= limit {
                    break;
                }
            }
        }
        if matches.len() >= limit {
            break;
        }
    }
    AgentResponse::ok(json!({"matches": matches, "count": matches.len()}))
}

fn load_entry(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &["collection"]) {
        return e;
    }
    let key = payload
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| payload.get("id").and_then(Value::as_str))
        .map(|s| sanitize_label(s, 200))
        .filter(|s| !s.is_empty());
    let Some(key) = key else {
        return AgentResponse::err(400, "missing 'name' or 'id'");
    };
    let coll_in = payload.get("collection").and_then(Value::as_str);
    let coll_name = if let Some(raw) = coll_in.filter(|s| !s.is_empty()) {
        if find_collection(session, raw).is_none() {
            return AgentResponse::err(404, format!("no collection named {raw:?}"));
        }
        Some(raw.to_owned())
    } else {
        None
    };
    let hits = scan_library(session, &key, coll_name.as_deref());
    if hits.is_empty() {
        let where_ = coll_name
            .as_deref()
            .map(|c| format!(" in collection {c:?}"))
            .unwrap_or_else(|| " in any collection".into());
        return AgentResponse::err(404, format!("no library entry matching {key:?}{where_}"));
    }
    let (holder, entry) = if hits.len() == 1 {
        hits.into_iter().next().expect("len 1")
    } else {
        let active = session.library.active.clone();
        let active_hits: Vec<_> = hits.iter().filter(|(c, _)| *c == active).cloned().collect();
        if active_hits.len() == 1 {
            active_hits.into_iter().next().expect("len 1")
        } else {
            let mut holders: Vec<String> = hits.into_iter().map(|(c, _)| c).collect();
            holders.sort();
            holders.dedup();
            return AgentResponse {
                status: 409,
                body: json!({
                    "error": format!(
                        "{key:?} matches entries in multiple collections ({}) — pass {{\"collection\": \"…\"}} to disambiguate",
                        holders.join(", ")
                    ),
                    "collections": holders,
                }),
            };
        }
    };
    if entry.gb_text.is_empty() {
        return AgentResponse::err(422, "library entry has no stored sequence");
    }
    let mut rec = match gb_text_to_record(&entry.gb_text) {
        Ok(r) => r,
        Err(e) => {
            return AgentResponse::err(
                500,
                format!("parse failed: {}", scrub_path(&e.to_string())),
            );
        }
    };
    rec.name = entry.name.clone();
    rec.id = entry.id.clone();
    persist::log_event(
        "agent.load-entry",
        &[("name", &rec.name), ("length", &rec.len().to_string())],
    );
    session.record = Some(rec);
    session.dirty = false;
    session.source_path = None;
    AgentResponse::ok(json!({
        "ok": true,
        "name": entry.name,
        "id": entry.id,
        "length": entry.size,
        "collection": holder,
    }))
}

fn list_features(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    let features: Vec<Value> = rec
        .features
        .iter()
        .map(|f| {
            json!({
                "label": f.label,
                "type": f.kind,
                "start": f.start,
                "end": f.end,
                "strand": f.strand,
            })
        })
        .collect();
    AgentResponse::ok(json!({"features": features, "count": features.len()}))
}

fn get_sequence(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    persist::log_event("agent.get-sequence", &[("length", &rec.len().to_string())]);
    AgentResponse::ok(json!({
        "sequence": rec.sequence,
        "length": rec.len(),
        "circular": rec.circular,
    }))
}

fn find_orfs_handler(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    for wrong in ["min_length", "min_bp", "min_len"] {
        if payload.get(wrong).is_some() {
            return AgentResponse::err(
                400,
                format!(
                    "unknown parameter {wrong:?}; find-orfs filters by \
                     amino-acid length — use 'min_aa' (amino acids, default 30). \
                     Note: ORF length here is in aa, not bp."
                ),
            );
        }
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    let min_aa = match coerce_int(payload.get("min_aa"), "min_aa") {
        Ok(None) => ORF_DEFAULT_MIN_AA as i64,
        Ok(Some(n)) if n < 1 => return AgentResponse::err(400, "'min_aa' must be ≥ 1"),
        Ok(Some(n)) => n,
        Err(e) => return e,
    };
    let alt = payload
        .get("include_alt_starts")
        .is_some_and(|v| matches!(v, Value::Bool(true)));
    let orfs = find_orfs(&rec.sequence, rec.circular, min_aa as usize, alt);
    persist::log_event(
        "agent.find-orfs",
        &[
            ("count", &orfs.len().to_string()),
            ("min_aa", &min_aa.to_string()),
        ],
    );
    let rows: Vec<Value> = orfs
        .iter()
        .map(|o| {
            json!({
                "start": o.start,
                "end": o.end,
                "strand": o.strand,
                "length_aa": o.length_aa,
                "nt_len": o.nt_len,
                "exceeds_one_lap": o.exceeds_one_lap,
                "aa_seq": o.aa_seq,
            })
        })
        .collect();
    AgentResponse::ok(json!({"orfs": rows, "count": rows.len()}))
}

fn list_restriction_sites(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &["enzyme", "enzymes"]) {
        return e;
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    let mut enzymes: Option<Vec<String>> = None;
    if let Some(v) = payload.get("enzymes") {
        match v {
            Value::String(s) => enzymes = Some(vec![s.clone()]),
            Value::Array(arr) => {
                let mut names = Vec::new();
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return AgentResponse::err(400, "'enzymes' must contain only strings");
                    };
                    names.push(s.to_owned());
                }
                enzymes = Some(names);
            }
            _ => {
                return AgentResponse::err(400, "'enzymes' must be a list (or 'enzyme' a string)");
            }
        }
    } else if let Some(s) = payload.get("enzyme").and_then(Value::as_str) {
        enzymes = Some(vec![s.to_owned()]);
    }
    let min_len = match coerce_int(payload.get("min_length"), "min_length") {
        Ok(None) => 4,
        Ok(Some(n)) if n < 1 => return AgentResponse::err(400, "'min_length' must be ≥ 1"),
        Ok(Some(n)) => n as usize,
        Err(e) => return e,
    };
    let unique = payload
        .get("unique_only")
        .is_some_and(|v| matches!(v, Value::Bool(true)));
    let opts = ScanOptions {
        min_recognition_len: min_len,
        unique_only: unique,
        circular: rec.circular,
        allowed_enzymes: enzymes,
        extra_enzymes: Vec::new(),
    };
    let hits = scan_restriction_sites(&rec.sequence, &opts);
    let sites: Vec<Value> = hits
        .iter()
        .filter(|h| h.is_resite())
        .map(|h| {
            json!({
                "enzyme": h.label,
                "start": h.start,
                "end": h.end,
                "strand": h.strand,
                "cut_bp": h.top_cut_bp,
            })
        })
        .collect();
    persist::log_event(
        "agent.list-restriction-sites",
        &[("count", &sites.len().to_string())],
    );
    AgentResponse::ok(json!({"sites": sites, "count": sites.len()}))
}

fn save(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = session.record.clone() else {
        return AgentResponse::err(422, "nothing to save");
    };
    let in_library = session
        .library
        .plasmids
        .iter()
        .any(|e| e.id == rec.id || e.name == rec.name);
    let create = payload
        .get("create")
        .is_some_and(|v| matches!(v, Value::Bool(true)));
    if !in_library && session.source_path.is_none() && !create {
        return AgentResponse {
            status: 409,
            body: json!({
                "error": format!(
                    "saving would CREATE a new library entry {:?} in collection {:?} — \
                     pass {{\"create\": true}} to confirm, or use add-current-to-library.",
                    rec.name, session.library.active
                ),
                "would_create": true,
                "name": rec.name,
                "collection": session.library.active,
            }),
        };
    }
    if let Err(e) = require_persist_writes() {
        return e;
    }
    let entry = match record_to_library_entry(&rec) {
        Ok(e) => e,
        Err(err) => {
            return AgentResponse::err(500, scrub_path(&err.to_string()));
        }
    };
    let choice = if in_library {
        Some(CollisionChoice::Overwrite)
    } else {
        None
    };
    match session.library.keep(entry, choice) {
        KeepOutcome::NeedsChoice { existing_name, .. } => {
            return AgentResponse {
                status: 409,
                body: json!({
                    "error": "name collision — pass choice skip|copy|overwrite",
                    "existing_name": existing_name,
                }),
            };
        }
        KeepOutcome::Cancelled => return AgentResponse::err(409, "keep cancelled"),
        KeepOutcome::Applied { .. } => {}
    }
    if let Err(err) = session.library.persist(&session.layout) {
        return persist_save_error("library", err);
    }
    session.dirty = false;
    persist::log_event("agent.save", &[("name", &rec.name)]);
    AgentResponse::ok(json!({
        "ok": true,
        "created": !in_library,
        "collection": session.library.active,
        "name": rec.name,
        "ignored": ignored_keys(payload, &["create"]),
    }))
}

fn add_current_to_library(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = session.record.clone() else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    if let Err(e) = require_persist_writes() {
        return e;
    }
    let mut entry = match record_to_library_entry(&rec) {
        Ok(e) => e,
        Err(err) => return AgentResponse::err(500, scrub_path(&err.to_string())),
    };
    if let Some(name) = payload.get("name").and_then(Value::as_str) {
        let cleaned = sanitize_label(name, 200);
        if !cleaned.is_empty() {
            entry.name = cleaned;
        }
    }
    if let Some(id) = payload.get("id").and_then(Value::as_str) {
        let cleaned: String = id
            .chars()
            .take(256)
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let cleaned = cleaned.trim_matches('_').to_owned();
        if cleaned.is_empty() {
            return AgentResponse::err(400, "invalid 'id' (empty after sanitising)");
        }
        entry.id = cleaned;
    }
    let present = session.library.plasmids.iter().any(|e| e.id == entry.id);
    let choice = match payload.get("choice").and_then(Value::as_str) {
        Some("skip") => Some(CollisionChoice::Skip),
        Some("copy") => Some(CollisionChoice::Copy),
        Some("overwrite") => Some(CollisionChoice::Overwrite),
        Some(other) => {
            return AgentResponse::err(400, format!("unknown choice {other:?}"));
        }
        None => None,
    };
    match session.library.keep(entry.clone(), choice) {
        KeepOutcome::NeedsChoice {
            class,
            existing_name,
        } => AgentResponse {
            status: 409,
            body: json!({
                "error": "name collision — pass choice skip|copy|overwrite",
                "class": format!("{class:?}").to_ascii_lowercase(),
                "existing_name": existing_name,
            }),
        },
        KeepOutcome::Cancelled => AgentResponse::err(409, "keep cancelled"),
        KeepOutcome::Applied { name } => {
            if let Err(err) = session.library.persist(&session.layout) {
                return persist_save_error("library", err);
            }
            persist::log_event("agent.add-current-to-library", &[("name", &name)]);
            AgentResponse::ok(json!({
                "ok": true,
                "name": name,
                "created": !present,
                "already_present": present,
                "collection": session.library.active,
                "ignored": ignored_keys(payload, &["id", "name", "choice"]),
            }))
        }
    }
}

fn delete_from_library(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(|s| sanitize_label(s, 200))
        .filter(|s| !s.is_empty());
    let Some(name) = name else {
        return AgentResponse::err(400, "missing 'name'");
    };
    if let Err(e) = require_persist_writes() {
        return e;
    }
    let idx = session.library.plasmids.iter().position(|e| e.name == name);
    let Some(idx) = idx else {
        return AgentResponse::err(404, format!("no entry named {name:?}"));
    };
    let removed = session.library.remove_at(idx);
    if let Err(err) = session.library.persist(&session.layout) {
        if let Some(entry) = removed {
            session.library.restore_at(idx, entry);
        }
        return persist_save_error("library", err);
    }
    if let Some(rec) = &session.record
        && (rec.name == name || removed.as_ref().is_some_and(|e| e.id == rec.id))
    {
        session.record = None;
        session.dirty = false;
        session.source_path = None;
    }
    persist::log_event("agent.delete-from-library", &[("name", &name)]);
    AgentResponse::ok(json!({"ok": true, "deleted": name}))
}

fn get_settings(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let map = load_settings_map(&session.layout);
    AgentResponse::ok(json!({
        "settings": map,
        "allow_online_search": allow_online_search(&session.layout),
    }))
}

fn set_setting(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(key) = payload.get("key").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing 'key'");
    };
    if key == SETTING_ALLOW_ONLINE_SEARCH || key == SETTING_ALLOW_ONLINE_LOOKUPS {
        return AgentResponse::err(403, "an agent cannot enable online search or lookups");
    }
    let Some(value) = payload.get("value") else {
        return AgentResponse::err(400, "missing 'value'");
    };
    let flag = match value {
        Value::Bool(b) => *b,
        _ => return AgentResponse::err(400, "'value' must be a boolean"),
    };
    if let Err(e) = require_persist_writes() {
        return e;
    }
    if let Err(err) = set_setting_bool(&session.layout, key, flag) {
        return persist_save_error("settings", err);
    }
    persist::log_event("agent.set-setting", &[("key", key)]);
    AgentResponse::ok(json!({"ok": true, "key": key, "value": flag}))
}

fn list_hmm_databases(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let dbs: Vec<Value> = load_hmm_catalog(&session.layout)
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "builtin": e.builtin,
                "format": e.format,
            })
        })
        .collect();
    AgentResponse::ok(json!({"databases": dbs, "count": dbs.len()}))
}

fn blast_local(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(query) = payload.get("query").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing or non-string 'query'");
    };
    if query.is_empty() {
        return AgentResponse::err(400, "missing or non-string 'query'");
    }
    let hint = match payload.get("program").and_then(Value::as_str) {
        Some(s) => match BlastProgram::parse(s) {
            Some(p) => p,
            None => {
                return AgentResponse::err(400, "'program' must be blastn, blastp, or hmmscan");
            }
        },
        None => BlastProgram::Blastn,
    };
    let (program, cleaned) = detect_query_program(query, hint);
    if cleaned.is_empty() {
        return AgentResponse::err(400, "query is empty after sanitisation");
    }
    let max_hits = match coerce_int(payload.get("max_hits"), "max_hits") {
        Ok(None) => 25,
        Ok(Some(n)) if n < 1 => return AgentResponse::err(400, "'max_hits' must be ≥ 1"),
        Ok(Some(n)) => n.min(100) as usize,
        Err(e) => return e,
    };
    let db = blast_db_from_library(&session.library, program, false);
    let hits = blast_search(&cleaned, &db, max_hits);
    persist::log_event(
        "agent.blast",
        &[
            ("program", program.as_str()),
            ("n_hits", &hits.len().to_string()),
            ("query_length", &cleaned.len().to_string()),
        ],
    );
    let rows: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "subject_id": h.subject_id,
                "subject_name": h.subject_name,
                "collection": h.subject_collection,
                "kind": h.kind,
                "strand": h.strand,
                "identity_pct": h.identity_pct,
                "score": h.score,
                "q_start": h.q_start,
                "q_end": h.q_end,
                "s_start": h.s_start,
                "s_end": h.s_end,
            })
        })
        .collect();
    AgentResponse::ok(json!({
        "ok": true,
        "program": program.as_str(),
        "query_length": cleaned.len(),
        "n_hits": rows.len(),
        "hits": rows,
    }))
}

fn blast_online(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    if !allow_online_search(&session.layout) {
        return online_refused();
    }
    let Some(query) = payload.get("query").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing or non-string 'query'");
    };
    let program = payload
        .get("program")
        .and_then(Value::as_str)
        .unwrap_or("blastn");
    let database = payload.get("database").and_then(Value::as_str);
    let max_hits = match coerce_int(payload.get("max_hits"), "max_hits") {
        Ok(None) => 25,
        Ok(Some(n)) => n.clamp(1, 100) as usize,
        Err(e) => return e,
    };
    persist::log_event(
        "agent.blast-online",
        &[
            ("program", program),
            ("query_length", &query.len().to_string()),
        ],
    );
    let cancel = CancellationToken::new();
    let transport = OfflineTransport;
    let policy = OnlineSearchPolicy {
        enabled: true,
        transport: &transport,
        cancel: &cancel,
        poll_interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_millis(1),
    };
    match ncbi_blast_online(query, program, database, max_hits, &policy) {
        Ok(hits) => AgentResponse::ok(json!({
            "ok": true,
            "program": program,
            "n_hits": hits.len(),
            "hits": hits.iter().map(|h| json!({
                "accession": h.accession,
                "description": h.description,
                "identity_pct": h.identity_pct,
            })).collect::<Vec<_>>(),
        })),
        Err(splicecraft_io::IoError::OnlineDisabled) => online_refused(),
        Err(e) => AgentResponse::err(
            502,
            format!("NCBI BLAST failed: {}", scrub_path(&e.to_string())),
        ),
    }
}

fn hmmscan_online(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    if !allow_online_search(&session.layout) {
        return online_refused();
    }
    let Some(query) = payload.get("query").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing or non-string 'query'");
    };
    let max_hits = match coerce_int(payload.get("max_hits"), "max_hits") {
        Ok(None) => 25,
        Ok(Some(n)) => n.clamp(1, 100) as usize,
        Err(e) => return e,
    };
    persist::log_event(
        "agent.hmmscan-online",
        &[("query_length", &query.len().to_string())],
    );
    let cancel = CancellationToken::new();
    let transport = OfflineTransport;
    let policy = OnlineSearchPolicy {
        enabled: true,
        transport: &transport,
        cancel: &cancel,
        poll_interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_millis(1),
    };
    match hmmer_web_hmmscan(query, max_hits, &policy) {
        Ok(hits) => AgentResponse::ok(json!({
            "ok": true,
            "n_hits": hits.len(),
            "hits": hits.iter().map(|h| json!({
                "acc": h.acc,
                "name": h.name,
            })).collect::<Vec<_>>(),
        })),
        Err(splicecraft_io::IoError::OnlineDisabled) => online_refused(),
        Err(e) => AgentResponse::err(502, format!("HMMER failed: {}", scrub_path(&e.to_string()))),
    }
}

fn load_file(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(raw) = payload.get("path").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing 'path'");
    };
    let path = match sanitize_agent_path(raw) {
        Ok(p) => p,
        Err(e) => return AgentResponse::err(400, e),
    };
    if let Err(e) = check_read_path(&path) {
        return AgentResponse::err(403, e);
    }
    let rec = match load_path(&path) {
        Ok(r) => r,
        Err(e) => return AgentResponse::err(400, scrub_path(&e.to_string())),
    };
    persist::log_event(
        "agent.load-file",
        &[("length", &rec.len().to_string()), ("name", &rec.name)],
    );
    let name = rec.name.clone();
    let length = rec.len();
    session.record = Some(rec);
    session.dirty = true;
    session.source_path = Some(path);
    AgentResponse::ok(json!({"ok": true, "name": name, "length": length}))
}

fn export_genbank(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    let Some(raw) = payload.get("path").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing 'path'");
    };
    let path = match sanitize_agent_path(raw) {
        Ok(p) => p,
        Err(e) => return AgentResponse::err(400, e),
    };
    if let Err(e) = check_write_path(&path) {
        return AgentResponse::err(403, e);
    }
    if let Err(e) = export_genbank_to_path(rec, &path) {
        return AgentResponse::err(
            500,
            format!("export failed: {}", scrub_path(&e.to_string())),
        );
    }
    persist::log_event(
        "agent.export-genbank",
        &[("length", &rec.len().to_string())],
    );
    AgentResponse::ok(json!({
        "ok": true,
        "path": scrub_path(&path.display().to_string()),
    }))
}

fn list_experiments(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let store = ExperimentStore::load(&session.layout);
    let entries: Vec<Value> = store
        .entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "title": e.title,
                "created_at": e.created_at,
                "updated_at": e.updated_at,
            })
        })
        .collect();
    AgentResponse::ok(json!({"experiments": entries, "count": entries.len()}))
}

fn list_primers(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let primers: Vec<Value> = load_primers(&session.layout)
        .entries
        .iter()
        .filter_map(|e| {
            let obj = e.as_object()?;
            Some(json!({
                "name": obj.get("name").and_then(Value::as_str).unwrap_or(""),
                "id": obj.get("id").and_then(Value::as_str).unwrap_or(""),
            }))
        })
        .collect();
    AgentResponse::ok(json!({"primers": primers, "count": primers.len()}))
}

fn list_gels(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let store = GelStore::load(&session.layout);
    let gels: Vec<Value> = store
        .entries
        .iter()
        .map(|g| json!({"id": g.id, "name": g.name}))
        .collect();
    AgentResponse::ok(json!({"gels": gels, "count": gels.len()}))
}

fn simulate_pcr_handler(session: &mut AgentSession, payload: &Value) -> AgentResponse {
    if let Err(e) = reject_dangerous(payload, &[]) {
        return e;
    }
    let Some(rec) = &session.record else {
        return AgentResponse::err(422, "no plasmid loaded");
    };
    let Some(fwd) = payload.get("forward").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing 'forward'");
    };
    let Some(rev) = payload.get("reverse").and_then(Value::as_str) else {
        return AgentResponse::err(400, "missing 'reverse'");
    };
    let max_amplicon = match coerce_int(payload.get("max_amplicon"), "max_amplicon") {
        Ok(None) => PCR_DEFAULT_MAX_AMPLICON as i64,
        Ok(Some(n)) => n,
        Err(e) => return e,
    };
    let amps = match simulate_pcr(&rec.sequence, fwd, rev, rec.circular, max_amplicon) {
        Ok(a) => a,
        Err(e) => return AgentResponse::err(400, e.to_string()),
    };
    persist::log_event("agent.simulate-pcr", &[("count", &amps.len().to_string())]);
    let rows: Vec<Value> = amps
        .iter()
        .map(|a| {
            json!({
                "start": a.start,
                "end": a.end,
                "length": a.length,
                "wraps": a.wraps,
                "gc_pct": a.gc_pct,
                "amplicon_seq": a.amplicon_seq,
            })
        })
        .collect();
    AgentResponse::ok(json!({"amplicons": rows, "count": rows.len()}))
}

fn online_refused() -> AgentResponse {
    AgentResponse::err(
        403,
        "online search is disabled until the allow_online_search setting is ticked",
    )
}

fn project_entry(e: &LibraryEntry) -> Value {
    json!({
        "name": e.name,
        "id": e.id,
        "length": e.size,
        "n_features": n_features(e),
        "topology": topology_from_gb(&e.gb_text),
        "source": e.source,
    })
}

fn n_features(e: &LibraryEntry) -> usize {
    if e.gb_text.is_empty() {
        return 0;
    }
    gb_text_to_record(&e.gb_text)
        .map(|r| r.features.len())
        .unwrap_or(0)
}

fn topology_from_gb(gb: &str) -> &'static str {
    let n = gb.len().min(200);
    if gb[..n].to_ascii_lowercase().contains("linear") {
        "linear"
    } else {
        "circular"
    }
}

fn find_collection<'a>(
    session: &'a AgentSession,
    name: &str,
) -> Option<&'a splicecraft_persist::Collection> {
    let n = name.trim();
    session
        .library
        .collections
        .iter()
        .find(|c| c.name == n)
        .or_else(|| {
            session
                .library
                .collections
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(n))
        })
}

fn scan_library(
    session: &AgentSession,
    key: &str,
    collection: Option<&str>,
) -> Vec<(String, LibraryEntry)> {
    let mut hits = Vec::new();
    for col in &session.library.collections {
        if let Some(want) = collection
            && col.name != want
            && !col.name.eq_ignore_ascii_case(want)
        {
            continue;
        }
        for e in &col.plasmids {
            if e.name == key || e.id == key {
                hits.push((col.name.clone(), e.clone()));
            }
        }
    }
    hits
}

fn fuzzy_text_match(query: &str, hay: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    let h = hay.to_ascii_lowercase();
    h.contains(&q) || is_subsequence(&q, &h)
}

fn is_subsequence(query: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    for qc in query.chars() {
        loop {
            match it.next() {
                Some(hc) if hc == qc => break,
                Some(_) => {}
                None => return false,
            }
        }
    }
    true
}

fn reject_dangerous(payload: &Value, allowed: &[&str]) -> Result<(), AgentResponse> {
    let Some(obj) = payload.as_object() else {
        return Ok(());
    };
    for key in obj.keys() {
        if key == "force" || allowed.contains(&key.as_str()) {
            continue;
        }
        if DANGEROUS.contains(&key.as_str()) {
            return Err(AgentResponse::err(
                400,
                format!("unknown routing parameter {key:?} for this endpoint"),
            ));
        }
    }
    Ok(())
}

fn ignored_keys(payload: &Value, known: &[&str]) -> Vec<String> {
    let Some(obj) = payload.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<String> = obj
        .keys()
        .filter(|k| *k != "force" && !known.contains(&k.as_str()))
        .cloned()
        .collect();
    out.sort();
    out
}

fn coerce_int(value: Option<&Value>, name: &str) -> Result<Option<i64>, AgentResponse> {
    let Some(v) = value else {
        return Ok(None);
    };
    if v.is_null() {
        return Ok(None);
    }
    match v {
        Value::Number(n) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| AgentResponse::err(400, format!("'{name}' must be an integer"))),
        Value::String(s) => s
            .parse::<i64>()
            .map(Some)
            .map_err(|_| AgentResponse::err(400, format!("'{name}' must be an integer"))),
        _ => Err(AgentResponse::err(
            400,
            format!("'{name}' must be an integer"),
        )),
    }
}
