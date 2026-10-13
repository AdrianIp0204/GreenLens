use super::*;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufRead, BufReader},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: String,
    pub label: String,
    pub unit: String,
}
fn channel(id: impl Into<String>, label: impl Into<String>, unit: &str) -> Channel {
    Channel {
        id: id.into(),
        label: label.into(),
        unit: unit.into(),
    }
}
pub fn channels(model: &ThermalModel) -> Vec<Channel> {
    let mut out = vec![
        channel("facility_j", "Electricity:Facility [J](TimeStep)", "J"),
        channel(
            "interior_lighting_j",
            "InteriorLights:Electricity [J](TimeStep)",
            "J",
        ),
        channel(
            "exterior_lighting_j",
            "ExteriorLights:Electricity [J](TimeStep)",
            "J",
        ),
        channel(
            "equipment_j",
            "InteriorEquipment:Electricity [J](TimeStep)",
            "J",
        ),
    ];
    if model.zones.iter().any(|z| z.cooling_system.is_some()) {
        out.push(channel(
            "cooling_j",
            "Cooling:Electricity [J](TimeStep)",
            "J",
        ));
        out.push(channel("fans_j", "Fans:Electricity [J](TimeStep)", "J"));
    }
    for (i, z) in model.zones.iter().enumerate() {
        let name = format!("Z{:03}", i + 1);
        let id = format!("zone_{:03}", i + 1);
        out.push(channel(
            format!("{id}_temperature_c"),
            format!("{name}:Zone Mean Air Temperature [C](TimeStep)"),
            "C",
        ));
        out.push(channel(
            format!("{id}_occupancy"),
            format!("{name}:Declared occupancy fraction"),
            "fraction",
        ));
        if let Some(dx) = &z.cooling_system {
            let schedule = model
                .schedules
                .iter()
                .position(|s| s.id == dx.cooling_setpoint_schedule)
                .map(|i| format!("SCH{:03}", i + 1))
                .unwrap_or_else(|| "INVALID_SCHEDULE".into());
            for (suffix, header, unit) in [
                (
                    "setpoint_c",
                    format!("{schedule}:Schedule Value [](TimeStep)"),
                    "C",
                ),
                (
                    "operative_setpoint_c",
                    format!("{name}:Zone Thermostat Cooling Setpoint Temperature [C](TimeStep)"),
                    "C",
                ),
                (
                    "supply_fan_j",
                    format!("{name} SUPPLY FAN:Fan Electricity Energy [J](TimeStep)"),
                    "J",
                ),
                (
                    "ventilation_fan_j",
                    format!("{name}:Zone Ventilation Fan Electricity Energy [J](TimeStep)"),
                    "J",
                ),
                (
                    "delivered_cooling_j",
                    format!("{name} COIL:Cooling Coil Total Cooling Energy [J](TimeStep)"),
                    "J",
                ),
            ] {
                out.push(channel(format!("{id}_{suffix}"), header, unit));
            }
        }
    }
    out
}
pub(super) fn native_header(c: &Channel) -> Option<&str> {
    if c.unit == "fraction" {
        None
    } else {
        Some(&c.label)
    }
}
pub fn ledger(model: &ThermalModel) -> Result<MeterLedger> {
    let c = channels(model);
    let mut entries = Vec::new();
    let mut add = |end_use, native_channels: Vec<String>, boundary: &str| {
        entries.push(MeterLedgerEntry {
            end_use,
            fuel: Fuel::Electricity,
            native_channels,
            boundary: boundary.into(),
            classification: ProcessClassification::Unclassified,
            classification_source: None,
        })
    };
    let native = |id: &str| c.iter().find(|c| c.id == id).unwrap().label.clone();
    add(EnergyEndUse::InteriorLighting,vec![native("interior_lighting_j")],"Interior lighting electricity, including declared return-air/radiant/visible heat fractions");
    add(
        EnergyEndUse::ExteriorLighting,
        vec![native("exterior_lighting_j")],
        "Exterior lighting electricity, fixed input schedule",
    );
    add(
        EnergyEndUse::PlugEquipment,
        vec![native("equipment_j")],
        "Interior electric equipment, with declared latent/radiant/lost fractions",
    );
    if model.zones.iter().any(|z| z.cooling_system.is_some()) {
        add(EnergyEndUse::CoolingSystem,vec![native("cooling_j")],"Air-cooled single-speed DX compressor/condenser electricity; gross COP excludes supply fan; heaters disabled");
        add(
            EnergyEndUse::SupplyFans,
            c.iter()
                .filter(|c| c.id.ends_with("_supply_fan_j"))
                .map(|c| c.label.clone())
                .collect(),
            "Explicit recirculating supply-fan electricity; continuous while AC available",
        );
        add(EnergyEndUse::VentilationExhaustFans,c.iter().filter(|c|c.id.ends_with("_ventilation_fan_j")).map(|c|c.label.clone()).collect(),"Separate balanced ventilation: combined equal intake/exhaust fan electricity, no heat recovery");
    }
    let out = MeterLedger {
        version: "greenlens.energyplus-disjoint-electricity.v1".into(),
        facility_native_channel: native("facility_j"),
        entries,
    };
    out.validate()?;
    Ok(out)
}
fn schedule_value(
    model: &ThermalModel,
    study: &EnergyStudy,
    id: &str,
    index: usize,
) -> Result<f64> {
    let s = model
        .schedules
        .iter()
        .find(|s| s.id == id)
        .context("Missing authoritative schedule")?;
    let factor = usize::from(study.zone_timesteps_per_hour / 6);
    ensure!(factor == 1 || factor == 2, "Unsupported timestep");
    let ten = index / factor;
    let day = ten / 144;
    let date = MonthDay::from_day_index(day)?;
    let profile = if let Some(h) = s.holidays.iter().find(|h| h.date == date) {
        &h.profile
    } else {
        match (study.calendar.start_weekday.index() + day) % 7 {
            5 => &s.saturday,
            6 => &s.sunday,
            _ => &s.weekday,
        }
    };
    profile
        .values
        .get(ten % 144)
        .copied()
        .context("Incomplete authoritative schedule")
}
pub fn decode_interval(
    model: &ThermalModel,
    study: &EnergyStudy,
    actual_channels: &[Channel],
    values: &[f64],
    index: usize,
) -> Result<EnergyInterval> {
    ensure!(
        actual_channels == channels(model),
        "Retained channel identity/order differs from compiled EnergyPlus map"
    );
    decode_checked(model, study, actual_channels, values, index)
}
/// Validated reusable retained-series decoder. Schedule lookup is one calendar
/// day and slot, never an annual expansion or model validation per interval.
pub struct Decoder {
    model: ThermalModel,
    study: EnergyStudy,
    pub channels: Vec<Channel>,
}
impl Decoder {
    pub fn new(model: &ThermalModel, study: &EnergyStudy) -> Result<Self> {
        study.validate(model)?;
        Ok(Self {
            model: model.clone(),
            study: study.clone(),
            channels: channels(model),
        })
    }
    pub fn decode(&self, index: usize, values: &[f64]) -> Result<EnergyInterval> {
        decode_checked(&self.model, &self.study, &self.channels, values, index)
    }
}
fn decode_checked(
    model: &ThermalModel,
    study: &EnergyStudy,
    c: &[Channel],
    values: &[f64],
    index: usize,
) -> Result<EnergyInterval> {
    ensure!(
        matches!(study.zone_timesteps_per_hour, 6 | 12)
            && index < 8760 * usize::from(study.zone_timesteps_per_hour)
            && values.len() == c.len(),
        "Energy row shape/index mismatch"
    );
    for (ch, &v) in c.iter().zip(values) {
        ensure!(
            v.is_finite()
                && (ch.unit != "J" || v >= 0.)
                && (ch.unit != "fraction" || (0.0..=1.).contains(&v)),
            "Invalid energy value in {}",
            ch.id
        );
    }
    let value = |id: &str| -> Result<f64> {
        let i = c
            .iter()
            .position(|c| c.id == id)
            .with_context(|| format!("Missing energy channel {id}"))?;
        Ok(values[i])
    };
    let mut end_uses_j = BTreeMap::from([
        (
            EnergyEndUse::InteriorLighting,
            value("interior_lighting_j")?,
        ),
        (
            EnergyEndUse::ExteriorLighting,
            value("exterior_lighting_j")?,
        ),
        (EnergyEndUse::PlugEquipment, value("equipment_j")?),
    ]);
    let (mut supply, mut ventilation) = (0., 0.);
    let mut comfort = Vec::new();
    for (i, z) in model.zones.iter().enumerate() {
        let prefix = format!("zone_{:03}", i + 1);
        let occupancy = value(&format!("{prefix}_occupancy"))?;
        ensure!(
            occupancy == schedule_value(model, study, &z.gains.occupancy_schedule, index)?,
            "Retained occupancy differs from explicit calendar schedule"
        );
        if let Some(dx) = &z.cooling_system {
            supply += value(&format!("{prefix}_supply_fan_j"))?;
            ventilation += value(&format!("{prefix}_ventilation_fan_j"))?;
            let setpoint = value(&format!("{prefix}_setpoint_c"))?;
            let expected = schedule_value(model, study, &dx.cooling_setpoint_schedule, index)?;
            ensure!(
                (setpoint - expected).abs() <= 1e-7,
                "Native cooling schedule differs at interval {index}, zone {}: actual {setpoint} C, expected {expected} C",
                z.id
            );
            // SingleCooling cutout hysteresis can lower the operative setpoint.
            // System substeps may average the two limits in a zone timestep.
            // Comfort is always evaluated against the independently verified schedule.
            let operative = value(&format!("{prefix}_operative_setpoint_c"))?;
            let lower = expected - dx.cooling_deadband_k;
            ensure!(
                operative >= lower - 1e-7 && operative <= expected + 1e-7,
                "Native operative cooling setpoint outside declared cutout range at interval {index}, zone {}: actual {operative} C, expected [{lower}, {expected}] C",
                z.id
            );
            comfort.push(ComfortSample {
                zone_id: z.id.clone(),
                occupied: occupancy > 0.,
                air_temperature_c: value(&format!("{prefix}_temperature_c"))?,
                cooling_setpoint_c: setpoint,
            });
        }
    }
    if !comfort.is_empty() {
        ensure!(
            supply.is_finite() && ventilation.is_finite(),
            "Fan aggregate overflow"
        );
        let fans = value("fans_j")?;
        let band = (3600. / f64::from(study.zone_timesteps_per_hour)).max(fans * 0.0001);
        ensure!(
            (fans - supply - ventilation).abs() <= band,
            "Supply and balanced-ventilation fans do not reconcile to Fans:Electricity"
        );
        end_uses_j.insert(EnergyEndUse::CoolingSystem, value("cooling_j")?);
        end_uses_j.insert(EnergyEndUse::SupplyFans, supply);
        end_uses_j.insert(EnergyEndUse::VentilationExhaustFans, ventilation);
    }
    Ok(EnergyInterval {
        index,
        facility_j: value("facility_j")?,
        end_uses_j,
        comfort,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostics {
    pub schema: String,
    pub completed: bool,
    pub warning_count: u32,
    pub severe_count: u32,
    pub messages: Vec<String>,
    pub convergence_disclosure: String,
    pub rows: usize,
    pub native_columns: usize,
    pub native_csv_sha256: String,
    pub native_csv_bytes: u64,
    pub error_file_sha256: String,
    pub end_file_sha256: String,
    pub input_sha256: BTreeMap<String, String>,
    pub minimum_values: Vec<f64>,
    pub maximum_values: Vec<f64>,
    pub maximum_fan_reconciliation_j: f64,
    pub qualification_asserted: bool,
}
fn bounded_text(path: &Path, max: u64) -> Result<String> {
    regular_file(path)?;
    ensure!(
        fs::metadata(path)?.len() <= max,
        "Diagnostic file exceeds limit"
    );
    Ok(fs::read_to_string(path)?)
}
fn summary(text: &str) -> Result<(u32, u32)> {
    let line = text
        .lines()
        .find(|l| l.contains("EnergyPlus Completed Successfully--"))
        .context("Missing exact successful EnergyPlus summary")?;
    let rest = line.split_once("Successfully--").unwrap().1;
    let mut parts = rest.split(';');
    let warnings = parts
        .next()
        .context("Warning summary missing")?
        .trim()
        .strip_suffix(" Warning")
        .context("Warning count missing")?
        .trim()
        .parse()?;
    let severe = parts
        .next()
        .context("Severe summary missing")?
        .trim()
        .strip_suffix(" Severe Errors")
        .context("Severe count missing")?
        .trim()
        .parse()?;
    Ok((warnings, severe))
}
fn line(reader: &mut impl BufRead) -> Result<Option<String>> {
    let mut out = Vec::new();
    loop {
        let b = reader.fill_buf()?;
        if b.is_empty() {
            break;
        }
        let n = b
            .iter()
            .position(|&v| v == b'\n')
            .map_or(b.len(), |i| i + 1);
        ensure!(out.len() + n <= 65536, "Native CSV line exceeds64KiB");
        let end = b[n - 1] == b'\n';
        out.extend_from_slice(&b[..n]);
        reader.consume(n);
        if end {
            break;
        }
    }
    if out.is_empty() {
        return Ok(None);
    };
    let value = String::from_utf8(out).context("Native CSV is not UTF-8")?;
    Ok(Some(value.trim_end_matches(['\r', '\n']).to_string()))
}
fn number(s: &str) -> Result<f64> {
    let s = s.trim();
    ensure!(
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)),
        "Invalid native numeric token"
    );
    let value: f64 = s.parse()?;
    ensure!(value.is_finite(), "Nonfinite native output");
    if value == 0. {
        let mantissa = s.split(['e', 'E']).next().unwrap();
        ensure!(
            !mantissa.bytes().any(|b| matches!(b, b'1'..=b'9')),
            "Native nonzero token underflow"
        );
    }
    Ok(value)
}
fn timestamp(index: usize, tph: u8) -> Result<String> {
    let per_day = 24 * usize::from(tph);
    let date = MonthDay::from_day_index(index / per_day)?;
    let minute = (index % per_day + 1) * (60 / usize::from(tph));
    Ok(format!(
        "{:02}/{:02}  {:02}:{:02}:00",
        date.month,
        date.day,
        minute / 60,
        minute % 60
    ))
}
pub(super) fn read_outputs<V>(
    p: &PreparedRun,
    cancel: &AtomicBool,
    visitor: &mut V,
) -> Result<Diagnostics>
where
    V: FnMut(&EnergyInterval, &[f64]) -> Result<()>,
{
    let directory = p.stage.join("engine");
    let err_path = directory.join("eplusout.err");
    let end_path = directory.join("eplusout.end");
    let csv = directory.join("eplusout.csv");
    let err = bounded_text(&err_path, 16 * 1024 * 1024)?;
    let end = bounded_text(&end_path, 65536)?;
    let (warnings, severe) = summary(&end)?;
    ensure!(
        summary(&err)? == (warnings, severe),
        "EnergyPlus error/end summary mismatch"
    );
    ensure!(
        severe == 0
            && !err.contains("** Severe **")
            && !err.contains("**  Fatal  **")
            && !err.contains("** Fatal **"),
        "EnergyPlus severe/fatal diagnostics prevent completion"
    );
    let lower = err.to_ascii_lowercase();
    ensure!(
        !lower.contains("did not converge") && !lower.contains("warmup convergence failure"),
        "Native convergence warning prevents completion"
    );
    let mut messages = Vec::new();
    let mut recording = false;
    for l in err.lines() {
        if l.contains("** Warning **") {
            recording = true;
        } else if !l.contains("**   ~~~   **") {
            recording = false;
        }
        if recording {
            messages.push(l.trim().into());
        }
    }
    regular_file(&csv)?;
    let bytes = fs::metadata(&csv)?.len();
    ensure!(
        bytes <= 134_217_728,
        "Native CSV file exceeds declared128MiB cap"
    );
    let mut reader = BufReader::new(File::open(&csv)?);
    let header = line(&mut reader)?.context("Missing native CSV header")?;
    let headers: Vec<_> = header.split(',').map(str::trim).collect();
    ensure!(
        headers.first() == Some(&"Date/Time")
            && headers.len() <= 65
            && headers.iter().copied().collect::<BTreeSet<_>>().len() == headers.len(),
        "Native CSV header/count/duplicates invalid"
    );
    let expected: BTreeSet<_> = p.channels.iter().filter_map(native_header).collect();
    ensure!(
        headers[1..].iter().copied().collect::<BTreeSet<_>>() == expected,
        "Missing or unexpected native output columns"
    );
    let indices: Vec<_> = p
        .channels
        .iter()
        .map(|c| native_header(c).map(|h| headers.iter().position(|v| *v == h).unwrap()))
        .collect();
    let expected_rows = 8760 * usize::from(p.study.zone_timesteps_per_hour);
    let mut row = 0;
    let mut min = vec![f64::INFINITY; p.channels.len()];
    let mut max = vec![f64::NEG_INFINITY; p.channels.len()];
    let mut max_fan = 0.0_f64;
    while let Some(l) = line(&mut reader)? {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Energy output parsing cancelled"
        );
        ensure!(row < expected_rows, "Extra native weather interval");
        let fields: Vec<_> = l.split(',').collect();
        ensure!(
            fields.len() == headers.len()
                && fields[0].trim() == timestamp(row, p.study.zone_timesteps_per_hour)?,
            "Native interval timestamp/order/column mismatch at{row}"
        );
        let mut values = Vec::with_capacity(p.channels.len());
        for (i, c) in p.channels.iter().enumerate() {
            let value = if let Some(k) = indices[i] {
                number(fields[k])?
            } else {
                let zone: usize = c.id.split('_').nth(1).context("Zone channel")?.parse()?;
                schedule_value(
                    &p.model,
                    &p.study,
                    &p.model.zones[zone - 1].gains.occupancy_schedule,
                    row,
                )?
            };
            min[i] = min[i].min(value);
            max[i] = max[i].max(value);
            values.push(value);
        }
        let interval = decode_checked(&p.model, &p.study, &p.channels, &values, row)?;
        if let Some(i) = p.channels.iter().position(|c| c.id == "fans_j") {
            let supply = interval.end_uses_j[&EnergyEndUse::SupplyFans];
            let vent = interval.end_uses_j[&EnergyEndUse::VentilationExhaustFans];
            max_fan = max_fan.max((values[i] - supply - vent).abs());
        }
        visitor(&interval, &values)?;
        row += 1;
    }
    ensure!(
        row == expected_rows,
        "Incomplete annual native output: {row}/{expected_rows}"
    );
    Ok(Diagnostics{schema:"greenlens.energy-diagnostics.v1".into(),completed:true,warning_count:warnings,severe_count:severe,messages,convergence_disclosure:"All native warnings are retained for review; severe/fatal or explicit convergence failures block completion. Native completion is not physical or timestep qualification.".into(),rows:row,native_columns:expected.len(),native_csv_sha256:file_hash(&csv)?,native_csv_bytes:bytes,error_file_sha256:file_hash(&err_path)?,end_file_sha256:file_hash(&end_path)?,input_sha256:p.inputs.clone(),minimum_values:min,maximum_values:max,maximum_fan_reconciliation_j:max_fan,qualification_asserted:false})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_tokens_and_summary() {
        for s in ["NaN", "Inf", "1e309", "1e-999", "", "0,1", "1x"] {
            assert!(number(s).is_err(), "{s}")
        }
        assert_eq!(number("-0.0e-999").unwrap(), 0.);
        assert_eq!(
            summary(
                "EnergyPlus Completed Successfully-- 2 Warning; 0 Severe Errors; Elapsed Time=x"
            )
            .unwrap(),
            (2, 0)
        );
        assert!(summary("success").is_err());
    }
    #[test]
    fn interval_end_axis() {
        assert_eq!(timestamp(0, 6).unwrap(), "01/01  00:10:00");
        assert_eq!(timestamp(143, 6).unwrap(), "01/01  24:00:00");
        assert_eq!(timestamp(105119, 12).unwrap(), "12/31  24:00:00");
    }
    #[test]
    fn bounded_line_rejects_oversized() {
        assert!(line(&mut BufReader::new(&vec![b'x'; 65537][..])).is_err());
    }
}
