use super::*;
use serde_json::Map;
use std::collections::BTreeMap;

fn add(m: &mut Value, kind: &str, name: impl AsRef<str>, fields: Value) {
    m.as_object_mut()
        .unwrap()
        .entry(kind)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap()
        .insert(name.as_ref().into(), fields);
}
fn native_names<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str) -> BTreeMap<String, String> {
    ids.enumerate()
        .map(|(i, id)| (id.into(), format!("{prefix}{:03}", i + 1)))
        .collect()
}
fn vertices(points: [[f64; 3]; 4]) -> Value {
    json!(points.map(|p|json!({"vertex_x_coordinate":p[0],"vertex_y_coordinate":p[1],"vertex_z_coordinate":p[2]})))
}
fn rotate(p: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.to_radians().sin_cos();
    [c * p[0] + s * p[1], -s * p[0] + c * p[1], p[2]]
}
fn face_axes(face: ThermalFace) -> (usize, [usize; 2], bool) {
    match face {
        ThermalFace::West => (0, [1, 2], false),
        ThermalFace::East => (0, [1, 2], true),
        ThermalFace::South => (1, [0, 2], false),
        ThermalFace::North => (1, [0, 2], true),
        ThermalFace::Floor => (2, [0, 1], false),
        ThermalFace::Roof => (2, [0, 1], true),
    }
}
fn rectangle(
    origin: [f64; 3],
    size: [f64; 3],
    face: ThermalFace,
    offset: [f64; 2],
    extent: [f64; 2],
) -> [[f64; 3]; 4] {
    let (a, [u, v], high) = face_axes(face);
    let mut p = origin;
    if high {
        p[a] += size[a]
    };
    p[u] += offset[0];
    p[v] += offset[1];
    let mut out = [p; 4];
    out[1][u] += extent[0];
    out[2][u] += extent[0];
    out[2][v] += extent[1];
    out[3][v] += extent[1];
    if (a != 1) != high {
        out.reverse()
    };
    out
}
pub(super) fn generate(model: &ThermalModel, study: &EnergyStudy) -> Result<Value> {
    let valid = validate_thermal_model(model, &study.calendar)?;
    let mut m = json!({});
    add(
        &mut m,
        "Version",
        "Version",
        json!({"version_identifier":"26.2"}),
    );
    add(
        &mut m,
        "Building",
        "GreenLens",
        json!({"north_axis":0,"terrain":"Suburbs","loads_convergence_tolerance_value":0.04,"temperature_convergence_tolerance_value":0.4,"solar_distribution":"FullExterior","maximum_number_of_warmup_days":50,"minimum_number_of_warmup_days":6}),
    );
    add(
        &mut m,
        "Timestep",
        "Timestep",
        json!({"number_of_timesteps_per_hour":study.zone_timesteps_per_hour}),
    );
    add(
        &mut m,
        "HeatBalanceAlgorithm",
        "Heat balance",
        json!({"algorithm":"ConductionTransferFunction"}),
    );
    add(
        &mut m,
        "SurfaceConvectionAlgorithm:Inside",
        "Inside",
        json!({"algorithm":"TARP"}),
    );
    add(
        &mut m,
        "SurfaceConvectionAlgorithm:Outside",
        "Outside",
        json!({"algorithm":"DOE-2"}),
    );
    add(
        &mut m,
        "ShadowCalculation",
        "Shadows",
        json!({"shading_calculation_method":"PolygonClipping","shading_calculation_update_frequency_method":"Periodic","shading_calculation_update_frequency":20,"maximum_figures_in_shadow_overlap_calculations":15000,"polygon_clipping_algorithm":"SutherlandHodgman","sky_diffuse_modeling_algorithm":"SimpleSkyDiffuseModeling"}),
    );
    add(
        &mut m,
        "ConvergenceLimits",
        "Convergence",
        json!({"minimum_system_timestep":1,"maximum_hvac_iterations":20,"minimum_plant_iterations":2,"maximum_plant_iterations":8}),
    );
    add(
        &mut m,
        "SimulationControl",
        "Simulation",
        json!({"do_zone_sizing_calculation":"No","do_system_sizing_calculation":"No","do_plant_sizing_calculation":"No","run_simulation_for_sizing_periods":"No","run_simulation_for_weather_file_run_periods":"Yes"}),
    );
    add(
        &mut m,
        "RunPeriod",
        "Annual",
        json!({"begin_month":1,"begin_day_of_month":1,"end_month":12,"end_day_of_month":31,"day_of_week_for_start_day":format!("{:?}",study.calendar.start_weekday),"use_weather_file_holidays_and_special_days":"No","use_weather_file_daylight_saving_period":"No","apply_weekend_holiday_rule":"No","use_weather_file_rain_indicators":"No","use_weather_file_snow_indicators":"No"}),
    );
    add(
        &mut m,
        "GlobalGeometryRules",
        "World",
        json!({"starting_vertex_position":"UpperLeftCorner","vertex_entry_direction":"Counterclockwise","coordinate_system":"World"}),
    );
    for (n, lo, hi, unit) in [
        ("Fraction", 0., 1., "Dimensionless"),
        ("Temperature", -50., 100., "Temperature"),
        ("Activity", 0., 2000., "ActivityLevel"),
        ("Control", 0., 4., "Control"),
    ] {
        add(
            &mut m,
            "ScheduleTypeLimits",
            n,
            json!({"lower_limit_value":lo,"upper_limit_value":hi,"numeric_type":"Continuous","unit_type":unit}),
        );
    }
    add(
        &mut m,
        "Schedule:Constant",
        "Always on",
        json!({"schedule_type_limits_name":"Fraction","hourly_value":1}),
    );
    add(
        &mut m,
        "Schedule:Constant",
        "Cooling control",
        json!({"schedule_type_limits_name":"Control","hourly_value":2}),
    );
    let schedules = native_names(model.schedules.iter().map(|s| s.id.as_str()), "SCH");
    let mut field_budget = 0;
    for s in &model.schedules {
        let expanded = expand_schedule(s, &study.calendar)?;
        let mut fields = Vec::new();
        let mut day = 0;
        while day < 365 {
            let values = &expanded[day * 144..(day + 1) * 144];
            let mut last = day;
            while last + 1 < 365 && expanded[(last + 1) * 144..(last + 2) * 144] == *values {
                last += 1;
            }
            let end = MonthDay::from_day_index(last)?;
            fields.extend([
                json!({"field":format!("Through: {}/{}",end.month,end.day)}),
                json!({"field":"For: AllDays"}),
                json!({"field":"Interpolate: No"}),
            ]);
            let mut slot = 0;
            while slot < 144 {
                let value = values[slot];
                let mut stop = slot + 1;
                while stop < 144 && values[stop] == value {
                    stop += 1
                }
                let minute = stop * 10;
                fields.push(json!({"field":format!("Until: {:02}:{:02}",minute/60,minute%60)}));
                fields.push(json!({"field":value}));
                slot = stop;
            }
            day = last + 1;
        }
        field_budget += fields.len();
        ensure!(field_budget<=1_500_000,"Expanded calendar schedules exceed bounded model representation; simplify daily variation");
        let typ = match s.kind {
            ScheduleKind::Fraction => "Fraction",
            ScheduleKind::ActivityWattsPerPerson => "Activity",
            ScheduleKind::TemperatureCelsius => "Temperature",
        };
        add(
            &mut m,
            "Schedule:Compact",
            &schedules[&s.id],
            json!({"schedule_type_limits_name":typ,"data":fields}),
        );
    }
    let materials = native_names(model.materials.iter().map(|v| v.id.as_str()), "MAT");
    for x in &model.materials {
        add(
            &mut m,
            "Material",
            &materials[&x.id],
            json!({"roughness":"MediumRough","thickness":x.thickness_m,"conductivity":x.conductivity_w_mk,"density":x.density_kg_m3,"specific_heat":x.specific_heat_j_kgk,"thermal_absorptance":x.thermal_absorptance,"solar_absorptance":x.solar_absorptance,"visible_absorptance":x.visible_absorptance}),
        );
    }
    let constructions = native_names(model.constructions.iter().map(|v| v.id.as_str()), "CON");
    for c in &model.constructions {
        let mut fields = Map::new();
        for (i, id) in c.layers.iter().enumerate() {
            fields.insert(
                if i == 0 {
                    "outside_layer".into()
                } else {
                    format!("layer_{}", i + 1)
                },
                json!(materials[id]),
            );
        }
        add(
            &mut m,
            "Construction",
            &constructions[&c.id],
            Value::Object(fields),
        );
    }
    let glazing = native_names(model.glazing.iter().map(|v| v.id.as_str()), "GLAZ");
    for g in &model.glazing {
        add(
            &mut m,
            "WindowMaterial:SimpleGlazingSystem",
            &glazing[&g.id],
            json!({"u_factor":g.u_value_w_m2k,"solar_heat_gain_coefficient":g.shgc,"visible_transmittance":g.visible_transmittance}),
        );
        add(
            &mut m,
            "Construction",
            format!("{}CON", glazing[&g.id]),
            json!({"outside_layer":glazing[&g.id]}),
        );
    }
    let curves = native_names(model.curves.iter().map(|v| v.id.as_str()), "CUR");
    for c in &model.curves {
        let (kind, fields) = match c.form {
            CurveForm::Quadratic { coefficients: k, x } => (
                "Curve:Quadratic",
                json!({"coefficient1_constant":k[0],"coefficient2_x":k[1],"coefficient3_x_2":k[2],"minimum_value_of_x":x[0],"maximum_value_of_x":x[1],"input_unit_type_for_x":"Dimensionless"}),
            ),
            CurveForm::Cubic { coefficients: k, x } => (
                "Curve:Cubic",
                json!({"coefficient1_constant":k[0],"coefficient2_x":k[1],"coefficient3_x_2":k[2],"coefficient4_x_3":k[3],"minimum_value_of_x":x[0],"maximum_value_of_x":x[1],"input_unit_type_for_x":"Dimensionless"}),
            ),
            CurveForm::Biquadratic {
                coefficients: k,
                x,
                y,
            } => (
                "Curve:Biquadratic",
                json!({"coefficient1_constant":k[0],"coefficient2_x":k[1],"coefficient3_x_2":k[2],"coefficient4_y":k[3],"coefficient5_y_2":k[4],"coefficient6_x_y":k[5],"minimum_value_of_x":x[0],"maximum_value_of_x":x[1],"minimum_value_of_y":y[0],"maximum_value_of_y":y[1],"input_unit_type_for_x":"Temperature","input_unit_type_for_y":"Temperature"}),
            ),
        };
        let mut fields = fields;
        fields["minimum_curve_output"] = json!(c.output_limits[0]);
        fields["maximum_curve_output"] = json!(c.output_limits[1]);
        fields["output_unit_type"] = json!("Dimensionless");
        add(&mut m, kind, &curves[&c.id], fields);
    }
    add(
        &mut m,
        "Curve:Quadratic",
        "Linear fan power",
        json!({"coefficient1_constant":0,"coefficient2_x":1,"coefficient3_x_2":0,"minimum_value_of_x":0,"maximum_value_of_x":1.5,"output_unit_type":"Dimensionless"}),
    );
    let zones = native_names(model.zones.iter().map(|v| v.id.as_str()), "Z");
    let surfaces = native_names(
        model
            .zones
            .iter()
            .flat_map(|z| z.surfaces.iter().map(|s| s.id.as_str())),
        "SUR",
    );
    for zone in &model.zones {
        let z = &zones[&zone.id];
        let area = zone.size_m[0] * zone.size_m[1];
        add(
            &mut m,
            "Zone",
            z,
            json!({"direction_of_relative_north":0,"x_origin":0,"y_origin":0,"z_origin":0,"type":1,"multiplier":1,"ceiling_height":zone.size_m[2],"volume":area*zone.size_m[2],"floor_area":area}),
        );
        for s in &zone.surfaces {
            let geom = valid
                .surfaces
                .iter()
                .find(|v| v.id == s.id)
                .context("Validated surface missing")?;
            let outside = matches!(s.boundary, ThermalBoundary::Outdoors);
            let mut f = json!({"surface_type":match s.face{ThermalFace::Floor=>"Floor",ThermalFace::Roof=>"Roof",_=>"Wall"},"construction_name":constructions[&s.construction_id],"zone_name":z,"outside_boundary_condition":match s.boundary{ThermalBoundary::Outdoors=>"Outdoors",ThermalBoundary::Interzone{..}=>"Surface",_=>"Adiabatic"},"sun_exposure":if outside{"SunExposed"}else{"NoSun"},"wind_exposure":if outside{"WindExposed"}else{"NoWind"},"view_factor_to_ground":"Autocalculate","number_of_vertices":4,"vertices":vertices(geom.vertices_m)});
            if let ThermalBoundary::Interzone { surface_id, .. } = &s.boundary {
                f["outside_boundary_condition_object"] = json!(surfaces[surface_id]);
            }
            add(&mut m, "BuildingSurface:Detailed", &surfaces[&s.id], f);
        }
        for (i, w) in zone.windows.iter().enumerate() {
            let s = zone
                .surfaces
                .iter()
                .find(|s| s.id == w.surface_id)
                .context("Window surface missing")?;
            let points = rectangle(
                zone.origin_m,
                zone.size_m,
                s.face,
                [w.offset_u_m, w.offset_v_m],
                [w.width_m, w.height_m],
            )
            .map(|p| rotate(p, model.north_rotation_deg));
            let mut f = json!({"surface_type":"Window","construction_name":format!("{}CON",glazing[&w.glazing_id]),"building_surface_name":surfaces[&w.surface_id],"number_of_vertices":4,"multiplier":1});
            for (j, p) in points.iter().enumerate() {
                for (k, axis) in ["x", "y", "z"].iter().enumerate() {
                    f[format!("vertex_{}_{}_coordinate", j + 1, axis)] = json!(p[k]);
                }
            }
            add(
                &mut m,
                "FenestrationSurface:Detailed",
                format!("{z}WIN{i:03}"),
                f,
            );
            if let Some(h) = &w.overhang {
                let (a, [u, v], high) = face_axes(s.face);
                ensure!(a != 2 && v == 2, "Only wall-window overhangs supported");
                let mut p = zone.origin_m;
                if high {
                    p[a] += zone.size_m[a]
                };
                p[u] += w.offset_u_m - h.side_extension_m;
                p[v] += w.offset_v_m + w.height_m + h.gap_above_m;
                let mut q = [p; 4];
                q[1][u] += w.width_m + 2. * h.side_extension_m;
                q[2] = q[1];
                q[2][a] += if high {
                    h.projection_m
                } else {
                    -h.projection_m
                };
                q[3][a] += if high {
                    h.projection_m
                } else {
                    -h.projection_m
                };
                add(
                    &mut m,
                    "Shading:Building:Detailed",
                    format!("{z}OH{i:03}"),
                    json!({"number_of_vertices":4,"vertices":vertices(q.map(|p|rotate(p,model.north_rotation_deg)))}),
                );
            }
        }
        let g = &zone.gains;
        add(
            &mut m,
            "People",
            format!("{z} People"),
            json!({"zone_or_zonelist_or_space_or_spacelist_name":z,"number_of_people_schedule_name":schedules[&g.occupancy_schedule],"number_of_people_calculation_method":"People","number_of_people":g.people_per_floor_m2*area,"fraction_radiant":g.people_radiant_fraction_of_sensible,"sensible_heat_fraction":g.people_sensible_fraction,"activity_level_schedule_name":schedules[&g.activity_schedule],"enable_ashrae_55_comfort_warnings":"No"}),
        );
        add(
            &mut m,
            "Lights",
            format!("{z} Lights"),
            json!({"zone_or_zonelist_or_space_or_spacelist_name":z,"schedule_name":schedules[&g.lighting_schedule],"design_level_calculation_method":"LightingLevel","lighting_level":g.lighting_w_per_floor_m2*area,"return_air_fraction":g.lighting_return_air_fraction,"fraction_radiant":g.lighting_radiant_fraction,"fraction_visible":g.lighting_visible_fraction}),
        );
        add(
            &mut m,
            "ElectricEquipment",
            format!("{z} Equipment"),
            json!({"zone_or_zonelist_or_space_or_spacelist_name":z,"schedule_name":schedules[&g.equipment_schedule],"design_level_calculation_method":"EquipmentLevel","design_level":g.equipment_w_per_floor_m2*area,"fraction_latent":g.equipment_latent_fraction,"fraction_radiant":g.equipment_radiant_fraction,"fraction_lost":g.equipment_lost_fraction}),
        );
        let flow = match g.infiltration {
            InfiltrationRate::AirChangesPerHour { value } => value * area * zone.size_m[2] / 3600.,
            InfiltrationRate::FlowM3PerSecond { value } => value,
            InfiltrationRate::FlowM3PerSecondPerFloorM2 { value } => value * area,
        };
        add(
            &mut m,
            "ZoneInfiltration:DesignFlowRate",
            format!("{z} Infiltration"),
            json!({"zone_or_zonelist_or_space_or_spacelist_name":z,"schedule_name":schedules[&g.infiltration_schedule],"design_flow_rate_calculation_method":"Flow/Zone","design_flow_rate":flow,"constant_term_coefficient":1,"temperature_term_coefficient":0,"velocity_term_coefficient":0,"velocity_squared_term_coefficient":0,"density_basis":"Standard"}),
        );
        if let Some(dx) = &zone.cooling_system {
            hvac(&mut m, z, dx, &schedules, &curves)?;
        }
    }
    for (i, shade) in model.detached_shading.iter().enumerate() {
        for (j, face) in ThermalFace::ALL.into_iter().enumerate() {
            let (_, uv, _) = face_axes(face);
            let p = rectangle(
                shade.origin_m,
                shade.size_m,
                face,
                [0., 0.],
                [shade.size_m[uv[0]], shade.size_m[uv[1]]],
            );
            add(
                &mut m,
                "Shading:Building:Detailed",
                format!("SHADE{i:03}F{j}"),
                json!({"number_of_vertices":4,"vertices":vertices(p.map(|p|rotate(p,model.north_rotation_deg)))}),
            );
        }
    }
    add(
        &mut m,
        "Exterior:Lights",
        "Exterior lights",
        json!({"schedule_name":schedules[&model.exterior_lighting_schedule],"design_level":model.exterior_lighting_w,"control_option":"ScheduleNameOnly"}),
    );
    let mut requested_headers = std::collections::BTreeSet::new();
    for c in channels(model) {
        if let Some(header) = output::native_header(&c) {
            // Multiple zones may use the same scheduled cooling setpoint.
            // One native schedule column is losslessly mapped to each zone.
            if !requested_headers.insert(header.to_owned()) {
                continue;
            }
            if let Some((key, var)) = header.split_once(':').filter(|_| c.id.starts_with("zone_")) {
                let var = var.split(" [").next().unwrap();
                add(
                    &mut m,
                    "Output:Variable",
                    &c.id,
                    json!({"key_value":key,"variable_name":var,"reporting_frequency":"Timestep"}),
                );
            } else {
                let name = header.split(" [").next().unwrap();
                add(
                    &mut m,
                    "Output:Meter",
                    &c.id,
                    json!({"key_name":name,"reporting_frequency":"Timestep"}),
                );
            }
        }
    }
    add(
        &mut m,
        "Output:VariableDictionary",
        "Dictionary",
        json!({"key_field":"IDF"}),
    );
    add(
        &mut m,
        "OutputControl:Files",
        "Files",
        json!({"output_csv":"Yes","output_mtr":"No","output_eso":"No","output_eio":"Yes","output_tabular":"No","output_sqlite":"No","output_json":"No","output_audit":"Yes","output_rdd":"Yes","output_mdd":"Yes","output_mtd":"Yes","output_end":"Yes","output_bnd":"Yes","output_screen":"Yes"}),
    );
    add(
        &mut m,
        "Output:Diagnostics",
        "Diagnostics",
        json!({"diagnostics":[{"key":"DisplayExtraWarnings"}]}),
    );
    Ok(m)
}
fn hvac(
    m: &mut Value,
    z: &str,
    d: &DxCoolingSystem,
    s: &BTreeMap<String, String>,
    curves: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!((d.rated_indoor_wet_bulb_c-19.44).abs()<1e-9&&(d.rated_outdoor_dry_bulb_c-35.).abs()<1e-9,"Single-speed DX requires rated19.44C indoor wet-bulb and35C outdoor dry-bulb; no silent rating conversion");
    let av = &s[&d.availability_schedule];
    let n = |suffix: &str| format!("{z} {suffix}");
    add(
        m,
        "ThermostatSetpoint:SingleCooling",
        n("Setpoint"),
        json!({"setpoint_temperature_schedule_name":s[&d.cooling_setpoint_schedule]}),
    );
    add(
        m,
        "ZoneControl:Thermostat",
        n("Thermostat"),
        json!({"zone_or_zonelist_name":z,"control_type_schedule_name":"Cooling control","control_1_object_type":"ThermostatSetpoint:SingleCooling","control_1_name":n("Setpoint"),"temperature_difference_between_cutout_and_setpoint":d.cooling_deadband_k}),
    );
    add(
        m,
        "ZoneHVAC:EquipmentConnections",
        n("Connections"),
        json!({"zone_name":z,"zone_conditioning_equipment_list_name":n("Equipment List"),"zone_air_inlet_node_or_nodelist_name":n("Supply"),"zone_air_exhaust_node_or_nodelist_name":n("Return"),"zone_air_node_name":n("Air")}),
    );
    add(
        m,
        "ZoneHVAC:EquipmentList",
        n("Equipment List"),
        json!({"load_distribution_scheme":"SequentialLoad","equipment":[{"zone_equipment_object_type":"ZoneHVAC:WindowAirConditioner","zone_equipment_name":n("AC"),"zone_equipment_cooling_sequence":1,"zone_equipment_heating_or_no_load_sequence":1}]}),
    );
    add(
        m,
        "OutdoorAir:Node",
        n("OA"),
        json!({"height_above_ground":0}),
    );
    add(
        m,
        "OutdoorAir:Mixer",
        n("Mixer"),
        json!({"mixed_air_node_name":n("Mixed"),"outdoor_air_stream_node_name":n("OA"),"relief_air_stream_node_name":n("Relief"),"return_air_stream_node_name":n("Return")}),
    );
    let fan = &d.supply_fan;
    add(
        m,
        "Fan:SystemModel",
        n("Supply Fan"),
        json!({"availability_schedule_name":av,"air_inlet_node_name":n("Mixed"),"air_outlet_node_name":n("Fan Out"),"design_maximum_air_flow_rate":d.rated_airflow_m3_s,"speed_control_method":"Continuous","electric_power_minimum_flow_rate_fraction":0,"design_pressure_rise":fan.pressure_rise_pa,"motor_efficiency":fan.motor_efficiency,"motor_in_air_stream_fraction":fan.motor_in_air_fraction,"design_electric_power_consumption":d.rated_airflow_m3_s*fan.pressure_rise_pa/fan.total_efficiency,"fan_total_efficiency":fan.total_efficiency,"electric_power_function_of_flow_fraction_curve_name":"Linear fan power","end_use_subcategory":"Supply"}),
    );
    add(
        m,
        "Coil:Cooling:DX:SingleSpeed",
        n("Coil"),
        json!({"availability_schedule_name":av,"gross_rated_total_cooling_capacity":d.gross_rated_capacity_w,"gross_rated_sensible_heat_ratio":d.rated_shr,"gross_rated_cooling_cop":d.gross_rated_cop,"rated_air_flow_rate":d.rated_airflow_m3_s,"air_inlet_node_name":n("Fan Out"),"air_outlet_node_name":n("Supply"),"total_cooling_capacity_function_of_temperature_curve_name":curves[&d.capacity_temperature_curve],"total_cooling_capacity_function_of_flow_fraction_curve_name":curves[&d.capacity_flow_curve],"energy_input_ratio_function_of_temperature_curve_name":curves[&d.eir_temperature_curve],"energy_input_ratio_function_of_flow_fraction_curve_name":curves[&d.eir_flow_curve],"part_load_fraction_correlation_curve_name":curves[&d.part_load_fraction_curve],"condenser_type":"AirCooled","crankcase_heater_capacity":0,"basin_heater_capacity":0,"minimum_outdoor_dry_bulb_temperature_for_compressor_operation":-20,"nominal_time_for_condensate_removal_to_begin":0,"ratio_of_initial_moisture_evaporation_rate_and_steady_state_latent_capacity":0}),
    );
    add(
        m,
        "ZoneHVAC:WindowAirConditioner",
        n("AC"),
        json!({"availability_schedule_name":av,"maximum_supply_air_flow_rate":d.rated_airflow_m3_s,"maximum_outdoor_air_flow_rate":0,"air_inlet_node_name":n("Return"),"air_outlet_node_name":n("Supply"),"outdoor_air_mixer_object_type":"OutdoorAir:Mixer","outdoor_air_mixer_name":n("Mixer"),"supply_air_fan_object_type":"Fan:SystemModel","supply_air_fan_name":n("Supply Fan"),"cooling_coil_object_type":"Coil:Cooling:DX:SingleSpeed","dx_cooling_coil_name":n("Coil"),"supply_air_fan_operating_mode_schedule_name":"Always on","fan_placement":"BlowThrough","cooling_convergence_tolerance":0.001}),
    );
    let v = &d.balanced_ventilation;
    add(
        m,
        "ZoneVentilation:DesignFlowRate",
        n("Balanced Ventilation"),
        json!({"zone_or_zonelist_or_space_or_spacelist_name":z,"schedule_name":s[&v.fraction_schedule],"design_flow_rate_calculation_method":"Flow/Zone","design_flow_rate":v.design_flow_m3_s,"ventilation_type":"Balanced","fan_pressure_rise":v.fan_pressure_pa,"fan_total_efficiency":v.total_efficiency,"constant_term_coefficient":1,"temperature_term_coefficient":0,"velocity_term_coefficient":0,"velocity_squared_term_coefficient":0,"minimum_indoor_temperature":-100,"maximum_indoor_temperature":100,"delta_temperature":-100,"minimum_outdoor_temperature":-100,"maximum_outdoor_temperature":100,"maximum_wind_speed":40,"density_basis":"Standard"}),
    );
    Ok(())
}
