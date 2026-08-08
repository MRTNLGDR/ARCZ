mod gltf;

use anyhow::{bail, Context, Result};
use arcz_determinism::{sha256_hex, Seed};
use arcz_procedural::input::GeneratorParameters;
use arcz_procedural::{generate, GeneratedArtifact};
use arcz_region::canonical_json;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerEnvelope {
    schema_version: u32,
    job_id: String,
    kind: String,
    generation_epoch: u64,
    request: Value,
    root: String,
    staging_dir: String,
}

fn main() {
    if let Err(error) = run_main() {
        eprintln!("ARCZ_GENERATION_ERROR: {error:#}");
        std::process::exit(1);
    }
}

fn run_main() -> Result<()> {
    let (request_path, output_dir) = parse_args()?;
    let raw = fs::read(&request_path).with_context(|| format!("ler {}", request_path.display()))?;
    let envelope: WorkerEnvelope = serde_json::from_slice(&raw).context("request envelope inválido")?;
    if envelope.schema_version != 1 { bail!("schema_version não suportado: {}", envelope.schema_version); }
    let root = PathBuf::from(&envelope.root).canonicalize().context("raiz ARCZ inválida")?;
    let output_dir = canonical_or_create(&output_dir)?;
    output_dir.strip_prefix(&root).context("output-dir precisa estar dentro da raiz ARCZ")?;
    let declared_staging = canonical_or_create(Path::new(&envelope.staging_dir))?;
    if output_dir != declared_staging { bail!("output-dir diverge de staging_dir declarado"); }
    progress(&output_dir, "VALIDATE_REQUEST", 0.04, "Validando contrato de entrada", None)?;

    let plugin_id = envelope.request.get("plugin_id").and_then(Value::as_str).unwrap_or("arcz.worker");
    let plugin_version = envelope.request.get("plugin_version").and_then(Value::as_str).unwrap_or("0.0.0");
    let request_hash = sha256_hex(canonical_json(&envelope.request));
    let profile_value = envelope.request.pointer("/region/profile").cloned()
        .or_else(|| envelope.request.pointer("/region/context").cloned()).unwrap_or_else(|| json!({}));
    let profile_hash = sha256_hex(canonical_json(&profile_value));
    let seed = explicit_or_derived_seed(&envelope.request, &request_hash)?;
    let generator = format!("{plugin_id}@{plugin_version}");

    progress(&output_dir, "ACQUIRE_INPUTS", 0.10, "Resolvendo somente entradas materializadas", None)?;
    let artifact = if envelope.kind == "region.context.generate" {
        let context = envelope.request.pointer("/region/context").cloned()
            .context("region.context.generate exige request.region.context materializado")?;
        GeneratedArtifact { scene: None, json: Some(context) }
    } else {
        let parameters_value = flattened_parameters(&envelope.request)?;
        let parameters: GeneratorParameters = serde_json::from_value(parameters_value)
            .context("parâmetros do gerador inválidos; consulte schemas/generators")?;
        progress(&output_dir, "GENERATE", 0.20, "Executando gramática procedural local", None)?;
        generate(&envelope.kind, parameters, seed).context("geração procedural falhou")?
    };

    progress(&output_dir, "VALIDATE_OUTPUT", 0.78, "Validando malhas e preparando artefatos", None)?;
    let mut outputs = Vec::new();
    let mut warnings = Vec::new();
    let mut metrics = Map::new();
    let mut provenance = Vec::new();
    if let Some(scene) = artifact.scene.as_ref() {
        let path = output_dir.join("generated.glb");
        gltf::write_glb(&path, scene).context("escrever GLB")?;
        outputs.push(output_record(&root, &path, "glb")?);
        warnings.extend(scene.warnings.clone());
        provenance.extend(scene.provenance.clone());
        let scene_metrics = scene.metrics();
        metrics.insert("scene".to_owned(), serde_json::to_value(&scene_metrics)?);
        metrics.insert("estimated_resources".to_owned(), serde_json::to_value(scene.estimated_resources())?);
    }
    if let Some(value) = artifact.json.as_ref() {
        let file_name = match envelope.kind.as_str() {
            "materials.generate" => "materials.json",
            "tiles.generate" => "tile-plan.json",
            "region.context.generate" => "region-context.json",
            _ => "result.json",
        };
        let path = output_dir.join(file_name);
        write_json(&path, value)?;
        outputs.push(output_record(&root, &path, "json")?);
    }
    if outputs.is_empty() { bail!("worker terminou sem artefatos"); }

    let report = json!({
        "schema_version":1,"job_id":envelope.job_id.clone(),"kind":envelope.kind.clone(),
        "generator":generator.clone(),"inputs_hash":request_hash.clone(),"profile_hash":profile_hash.clone(),
        "seed":seed,"generation_epoch":envelope.generation_epoch,"warnings":warnings.clone(),
        "metrics":metrics.clone(),"provenance":provenance.clone(),
        "network_mode":"offline_strict","deterministic":true
    });
    let report_path = output_dir.join("generation-report.json");
    write_json(&report_path, &report)?;
    outputs.push(output_record(&root, &report_path, "report")?);

    progress(&output_dir, "PERSIST", 0.94, "Gravando manifest e checksums", Some(json!({"outputs":outputs.len()})))?;
    let source_versions = source_versions(&envelope.request);
    let manifest = json!({
        "schema_version":1,"job_id":envelope.job_id,"generator":generator,
        "inputs_hash":request_hash,"profile_hash":profile_hash,"seed":seed,
        "source_versions":source_versions,"outputs":outputs,"warnings":warnings,
        "metrics":metrics,"created_at":utc_now(),"deterministic":true,
        "generation_epoch":envelope.generation_epoch
    });
    write_json(&output_dir.join("manifest.json"), &manifest)?;
    progress(&output_dir, "DONE", 1.0, "Geração concluída", None)?;
    Ok(())
}

fn parse_args() -> Result<(PathBuf, PathBuf)> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("run") { bail!("uso: arcz-generation-cli run --request <json> --output-dir <dir>"); }
    let mut request = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--request" => request = args.next().map(PathBuf::from),
            "--output-dir" => output = args.next().map(PathBuf::from),
            other => bail!("argumento desconhecido: {other}"),
        }
    }
    Ok((request.context("--request ausente")?, output.context("--output-dir ausente")?))
}

fn flattened_parameters(request: &Value) -> Result<Value> {
    let mut params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let object = params.as_object_mut().context("request.params precisa ser objeto")?;
    if let Some(inputs) = object.remove("inputs") {
        let inputs = inputs.as_object().context("params.inputs precisa ser objeto")?;
        for (key, value) in inputs {
            if object.contains_key(key) { bail!("campo duplicado em params e params.inputs: {key}"); }
            object.insert(key.clone(), value.clone());
        }
    }
    object.remove("seed");
    Ok(params)
}

fn explicit_or_derived_seed(request: &Value, request_hash: &str) -> Result<u64> {
    if let Some(value) = request.pointer("/params/seed") {
        return value.as_u64().context("params.seed precisa ser inteiro sem sinal");
    }
    let raw = u64::from_str_radix(&request_hash[..16], 16).context("derivar seed")?;
    Ok(Seed(raw).derive("arcz-generation", request_hash.as_bytes()).0)
}

fn source_versions(request: &Value) -> Value {
    if let Some(value) = request.get("source_versions").filter(|value| value.is_object()) { return value.clone(); }
    let mut result = Map::new();
    if let Some(values) = request.pointer("/region/context/source_packages").and_then(Value::as_array) {
        for value in values.iter().filter_map(Value::as_str) { result.insert(value.to_owned(), json!("materialized")); }
    }
    Value::Object(result)
}

fn canonical_or_create(path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(path).with_context(|| format!("criar {}", path.display()))?;
    path.canonicalize().with_context(|| format!("canonicalizar {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(value)?;
    let temporary = path.with_extension(format!("{}.partial", path.extension().and_then(|v| v.to_str()).unwrap_or("json")));
    {
        let mut file = fs::File::create(&temporary).with_context(|| format!("criar {}", temporary.display()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    replace_file(&temporary, path).with_context(|| format!("commit atômico {}", path.display()))?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::ffi::OsStr;
    use std::iter;
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new_name: *const u16, flags: u32) -> i32;
    }
    fn wide(value: &OsStr) -> Vec<u16> { value.encode_wide().chain(iter::once(0)).collect() }
    let source = wide(source.as_os_str());
    let destination = wide(destination.as_os_str());
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(),
        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if result == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

fn progress(output_dir: &Path, stage: &str, value: f64, message: &str, metrics: Option<Value>) -> Result<()> {
    let payload = json!({"stage":stage,"progress":value,"message":message,"metrics":metrics.unwrap_or_else(||json!({}))});
    write_json(&output_dir.join("progress.json"), &payload)
}

fn output_record(root: &Path, path: &Path, kind: &str) -> Result<Value> {
    let relative = path.strip_prefix(root).context("artefato fora da raiz")?.to_string_lossy().replace('\\', "/");
    let metadata = fs::metadata(path)?;
    Ok(json!({"path":relative,"sha256":sha256_file(path)?,"bytes":metadata.len(),"kind":kind}))
}

fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 { break; }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn utc_now() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epoch_e_utc() { assert_eq!(civil_from_days(0), (1970,1,1)); }
    #[test]
    fn leap_day() { assert_eq!(civil_from_days(18_321), (2020,2,29)); }
}
