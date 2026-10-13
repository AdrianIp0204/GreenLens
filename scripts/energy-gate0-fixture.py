"""Original bounded native feasibility inputs; Python is a development tool only.

This does not participate in application execution or accept user epJSON.
"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path

CASES = {"electrical": (1, 365), "envelope": (1, 7), "dx": (1, 3), "resource8": (8, 365)}


def make(case, root):
    zones, days = CASES[case]
    root.mkdir(parents=True, exist_ok=False)
    model = {}

    def add(kind, name, **fields):
        model.setdefault(kind, {})[name] = fields

    add("Version", "Version", version_identifier="26.2")
    add("Building", "Original GreenLens Gate 0", north_axis=0, terrain="Suburbs", loads_convergence_tolerance_value=.04, temperature_convergence_tolerance_value=.4, solar_distribution="FullExterior", maximum_number_of_warmup_days=50, minimum_number_of_warmup_days=6)
    add("Timestep", "Timestep", number_of_timesteps_per_hour=6)
    add("SimulationControl", "Simulation", do_zone_sizing_calculation="No", do_system_sizing_calculation="No", do_plant_sizing_calculation="No", run_simulation_for_sizing_periods="No", run_simulation_for_weather_file_run_periods="Yes")
    end = dt.date(2025, 1, 1) + dt.timedelta(days=days-1)
    add("RunPeriod", "Weather only", begin_month=1, begin_day_of_month=1, end_month=end.month, end_day_of_month=end.day, day_of_week_for_start_day="Wednesday", use_weather_file_holidays_and_special_days="No", use_weather_file_daylight_saving_period="No", apply_weekend_holiday_rule="No", use_weather_file_rain_indicators="No", use_weather_file_snow_indicators="No")
    add("GlobalGeometryRules", "Geometry", starting_vertex_position="UpperLeftCorner", vertex_entry_direction="Counterclockwise", coordinate_system="World")
    add("ScheduleTypeLimits", "Fraction", lower_limit_value=0, upper_limit_value=1, numeric_type="Continuous", unit_type="Dimensionless")
    add("ScheduleTypeLimits", "Temperature", lower_limit_value=-100, upper_limit_value=100, numeric_type="Continuous", unit_type="Temperature")
    add("ScheduleTypeLimits", "Control", lower_limit_value=0, upper_limit_value=4, numeric_type="Discrete", unit_type="Control")
    for name, value, typ in [("On",1,"Fraction"),("Off",0,"Fraction"),("Half",.5,"Fraction"),("Cooling",25 if case=="envelope" else 24,"Temperature"),("Control",2,"Control")]:
        add("Schedule:Constant", name, schedule_type_limits_name=typ, hourly_value=value)
    add("Schedule:Compact", "HVAC", schedule_type_limits_name="Fraction", data=[{"field": x} for x in ["Through: 12/31", "For: AllDays", "Until: 06:00", 0, "Until: 24:00", 1]])
    add("Material:NoMass", "Insulation", roughness="Smooth", thermal_resistance=2.0, thermal_absorptance=1e-9 if case=="envelope" else .9, solar_absorptance=0 if case=="envelope" else .6, visible_absorptance=.6)
    add("Construction", "Wall", outside_layer="Insulation")
    add("WindowMaterial:SimpleGlazingSystem", "Glazing", u_factor=2.5, solar_heat_gain_coefficient=.4, visible_transmittance=.6)
    add("Construction", "Window", outside_layer="Glazing")
    occupied = {}
    for i in range(zones):
        ix, iy, iz = (i % 2, (i // 2) % 2, i // 4) if zones > 1 else (0,0,0)
        occupied[(ix,iy,iz)] = f"Z{i+1}"
    for xyz, zone in occupied.items():
        ix,iy,iz=xyz; x,y,z=ix*4,iy*5,iz*3
        add("Zone", zone, direction_of_relative_north=0, x_origin=0, y_origin=0, z_origin=0, type=1, multiplier=1, ceiling_height=3, volume=60, floor_area=20)
        faces = {
            "S": ("Wall", (0,-1,0), [(x,y,z+3),(x,y,z),(x+4,y,z),(x+4,y,z+3)]),
            "N": ("Wall", (0,1,0), [(x+4,y+5,z+3),(x+4,y+5,z),(x,y+5,z),(x,y+5,z+3)]),
            "W": ("Wall", (-1,0,0), [(x,y+5,z+3),(x,y+5,z),(x,y,z),(x,y,z+3)]),
            "E": ("Wall", (1,0,0), [(x+4,y,z+3),(x+4,y,z),(x+4,y+5,z),(x+4,y+5,z+3)]),
            "F": ("Floor", (0,0,-1), [(x,y,z),(x,y+5,z),(x+4,y+5,z),(x+4,y,z)]),
            "R": ("Roof", (0,0,1), [(x,y+5,z+3),(x,y,z+3),(x+4,y,z+3),(x+4,y+5,z+3)])}
        opposite={"S":"N","N":"S","W":"E","E":"W","F":"R","R":"F"}
        for f,(kind, offset,vertices) in faces.items():
            adjacent = occupied.get(tuple(a+b for a,b in zip(xyz,offset)))
            boundary = "Surface" if adjacent else "Adiabatic" if f=="F" or (case=="envelope" and f!="S") else "Outdoors"
            fields=dict(surface_type=kind, construction_name="Wall", zone_name=zone, outside_boundary_condition=boundary, sun_exposure="SunExposed" if boundary=="Outdoors" else "NoSun", wind_exposure="WindExposed" if boundary=="Outdoors" else "NoWind", view_factor_to_ground=.5, number_of_vertices=4, vertices=[dict(vertex_x_coordinate=a,vertex_y_coordinate=b,vertex_z_coordinate=c) for a,b,c in vertices])
            if adjacent: fields["outside_boundary_condition_object"]=adjacent+opposite[f]
            add("BuildingSurface:Detailed", zone+f, **fields)
            if case=="envelope":
                add("SurfaceProperty:ConvectionCoefficients", zone+f+" Films", surface_name=zone+f, convection_coefficient_1_location="Inside", convection_coefficient_1_type="Value", convection_coefficient_1=10, convection_coefficient_2_location="Outside", convection_coefficient_2_type="Value", convection_coefficient_2=25)
        if case=="resource8" and iy==0:
            verts=[(x+1,y,z+2.5),(x+1,y,z+1),(x+3,y,z+1),(x+3,y,z+2.5)]
            wf=dict(surface_type="Window", construction_name="Window", building_surface_name=zone+"S", number_of_vertices=4)
            for j,(a,b,c) in enumerate(verts,1):
                wf.update({f"vertex_{j}_x_coordinate":a,f"vertex_{j}_y_coordinate":b,f"vertex_{j}_z_coordinate":c})
            add("FenestrationSurface:Detailed",zone+" Window",**wf)
        if case!="envelope":
            add("ElectricEquipment",zone+" Equipment",zone_or_zonelist_or_space_or_spacelist_name=zone,schedule_name="Half",design_level_calculation_method="EquipmentLevel",design_level=1000,fraction_latent=0,fraction_radiant=0,fraction_lost=0)
            add("Lights",zone+" Lights",zone_or_zonelist_or_space_or_spacelist_name=zone,schedule_name="Half",design_level_calculation_method="LightingLevel",lighting_level=500,return_air_fraction=0,fraction_radiant=0,fraction_visible=0)
        if case!="electrical":
            add("ThermostatSetpoint:SingleCooling",zone+" Setpoint",setpoint_temperature_schedule_name="Cooling")
            add("ZoneControl:Thermostat",zone+" Thermostat",zone_or_zonelist_name=zone,control_type_schedule_name="Control",control_1_object_type="ThermostatSetpoint:SingleCooling",control_1_name=zone+" Setpoint")
            equipment_type="ZoneHVAC:IdealLoadsAirSystem" if case=="envelope" else "ZoneHVAC:WindowAirConditioner"
            add("ZoneHVAC:EquipmentConnections",zone+" Connections",zone_name=zone,zone_conditioning_equipment_list_name=zone+" Equipment List",zone_air_inlet_node_or_nodelist_name=zone+" Supply",zone_air_exhaust_node_or_nodelist_name=zone+" Return",zone_air_node_name=zone+" Air")
            add("ZoneHVAC:EquipmentList",zone+" Equipment List",load_distribution_scheme="SequentialLoad",equipment=[dict(zone_equipment_object_type=equipment_type,zone_equipment_name=zone+" AC",zone_equipment_cooling_sequence=1,zone_equipment_heating_or_no_load_sequence=1)])
            if case=="envelope":
                add(equipment_type,zone+" AC",availability_schedule_name="On",zone_supply_air_node_name=zone+" Supply",zone_exhaust_air_node_name=zone+" Return",maximum_heating_supply_air_temperature=50,minimum_cooling_supply_air_temperature=13,heating_limit="LimitCapacity",maximum_sensible_heating_capacity=0,cooling_limit="NoLimit",heating_availability_schedule_name="Off",cooling_availability_schedule_name="On",dehumidification_control_type="None",humidification_control_type="None")
            else:
                add("OutdoorAir:Node",zone+" OA",height_above_ground=0)
                add("OutdoorAir:Mixer",zone+" Mixer",mixed_air_node_name=zone+" Mixed",outdoor_air_stream_node_name=zone+" OA",relief_air_stream_node_name=zone+" Relief",return_air_stream_node_name=zone+" Return")
                add("Fan:SystemModel",zone+" Supply Fan",availability_schedule_name="HVAC",air_inlet_node_name=zone+" Mixed",air_outlet_node_name=zone+" Fan Out",design_maximum_air_flow_rate=.35,speed_control_method="Continuous",electric_power_minimum_flow_rate_fraction=0,design_pressure_rise=100,motor_efficiency=.9,motor_in_air_stream_fraction=1,design_electric_power_consumption=70,fan_total_efficiency=.5,electric_power_function_of_flow_fraction_curve_name="Fan Power",end_use_subcategory="Supply")
                add("Coil:Cooling:DX:SingleSpeed",zone+" Coil",availability_schedule_name="HVAC",gross_rated_total_cooling_capacity=7000,gross_rated_sensible_heat_ratio=.75,gross_rated_cooling_cop=3,rated_air_flow_rate=.35,air_inlet_node_name=zone+" Fan Out",air_outlet_node_name=zone+" Supply",total_cooling_capacity_function_of_temperature_curve_name="Capacity T",total_cooling_capacity_function_of_flow_fraction_curve_name="Flow",energy_input_ratio_function_of_temperature_curve_name="EIR T",energy_input_ratio_function_of_flow_fraction_curve_name="Flow",part_load_fraction_correlation_curve_name="PLF",condenser_type="AirCooled",crankcase_heater_capacity=0,minimum_outdoor_dry_bulb_temperature_for_compressor_operation=-20,nominal_time_for_condensate_removal_to_begin=0,ratio_of_initial_moisture_evaporation_rate_and_steady_state_latent_capacity=0)
                add(equipment_type,zone+" AC",availability_schedule_name="HVAC",maximum_supply_air_flow_rate=.35,maximum_outdoor_air_flow_rate=0,air_inlet_node_name=zone+" Return",air_outlet_node_name=zone+" Supply",outdoor_air_mixer_object_type="OutdoorAir:Mixer",outdoor_air_mixer_name=zone+" Mixer",supply_air_fan_object_type="Fan:SystemModel",supply_air_fan_name=zone+" Supply Fan",cooling_coil_object_type="Coil:Cooling:DX:SingleSpeed",dx_cooling_coil_name=zone+" Coil",supply_air_fan_operating_mode_schedule_name="On",fan_placement="BlowThrough",cooling_convergence_tolerance=.001)
                add("ZoneVentilation:DesignFlowRate",zone+" Balanced Ventilation",zone_or_zonelist_or_space_or_spacelist_name=zone,schedule_name="On",design_flow_rate_calculation_method="Flow/Zone",design_flow_rate=.02,ventilation_type="Balanced",fan_pressure_rise=50,fan_total_efficiency=.5,constant_term_coefficient=1,temperature_term_coefficient=0,velocity_term_coefficient=0,velocity_squared_term_coefficient=0,minimum_indoor_temperature=-100,maximum_indoor_temperature=100,delta_temperature=-100,minimum_outdoor_temperature=-100,maximum_outdoor_temperature=100,maximum_wind_speed=40,density_basis="Standard")
    if case in ("dx","resource8"):
        for name in ["Capacity T","EIR T"]:
            add("Curve:Biquadratic",name,coefficient1_constant=1,coefficient2_x=0,coefficient3_x_2=0,coefficient4_y=0,coefficient5_y_2=0,coefficient6_x_y=0,minimum_value_of_x=0,maximum_value_of_x=50,minimum_value_of_y=-20,maximum_value_of_y=60,input_unit_type_for_x="Temperature",input_unit_type_for_y="Temperature",output_unit_type="Dimensionless")
        for name,coefs in [("Flow",(1,0,0)),("PLF",(.85,.15,0)),("Fan Power",(0,1,0))]:
            add("Curve:Quadratic",name,coefficient1_constant=coefs[0],coefficient2_x=coefs[1],coefficient3_x_2=coefs[2],minimum_value_of_x=0,maximum_value_of_x=1.5,output_unit_type="Dimensionless")
    if case!="envelope": add("Exterior:Lights","Exterior Lights",schedule_name="On",design_level=50,control_option="ScheduleNameOnly")
    meters=[] if case=="envelope" else ["Electricity:Facility","InteriorLights:Electricity","ExteriorLights:Electricity","InteriorEquipment:Electricity"]
    if case in ("dx","resource8"): meters += ["Cooling:Electricity","Fans:Electricity"]
    for meter in meters: add("Output:Meter",meter,key_name=meter,reporting_frequency="Timestep")
    variables=[("*","Zone Mean Air Temperature")]
    if case!="electrical":variables += [("*","Zone Thermostat Cooling Setpoint Temperature")]
    if case in ("dx","resource8"):
        variables += [("*","Fan Electricity Energy"),("*","Zone Ventilation Fan Electricity Energy"),("*","Cooling Coil Total Cooling Energy")]
        if case=="dx": variables += [("*","Cooling Coil Electricity Energy"),("*","Zone Ventilation Standard Density Volume Flow Rate"),("*","Zone Window Air Conditioner Electricity Energy")]
    if case=="envelope":variables += [("Z1S","Surface Inside Face Conduction Heat Transfer Rate"),("Z1S","Surface Inside Face Temperature"),("Z1S","Surface Outside Face Temperature"),("*","Zone Ideal Loads Supply Air Total Cooling Rate")]
    for i,(key,var) in enumerate(variables):add("Output:Variable",f"V{i}",key_value=key,variable_name=var,reporting_frequency="Timestep")
    add("Output:VariableDictionary","Dictionary",key_field="IDF")
    add("OutputControl:Files","Files",output_csv="Yes",output_mtr="No",output_eso="No",output_eio="Yes",output_tabular="No",output_sqlite="No",output_json="No",output_audit="Yes",output_rdd="Yes",output_mdd="Yes",output_mtd="Yes",output_end="Yes",output_bnd="Yes",output_screen="Yes")
    add("Output:Diagnostics","Diagnostics",diagnostics=[{"key":"DisplayExtraWarnings"}])
    model_bytes=(json.dumps(model,indent=2,allow_nan=False)+"\n").encode()
    (root/"model.epJSON").write_bytes(model_bytes)
    headers=["LOCATION,Original Synthetic,NA,NA,GreenLens,000000,22.3,114.17,8,0","DESIGN CONDITIONS,0","TYPICAL/EXTREME PERIODS,0","GROUND TEMPERATURES,0","HOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0","COMMENTS 1,Original Apache-2.0 synthetic constant weather; not measured climate","COMMENTS 2,35C dry bulb 20C dew point 41percent RH zero solar constant infrared400 wind2","DATA PERIODS,1,1,Data,Wednesday,1/1,12/31"]
    rows=[]
    for day in range(365):
        date=dt.date(2025,1,1)+dt.timedelta(days=day)
        for hour in range(1,25):
            row=[2025,date.month,date.day,hour,60,"?9?9?9?9",35,20,41,101325,0,0,400,0,0,0,0,0,0,0,180,2,10,10,20,77777,9,999999999,20,.1,0,99,.2,0,0]
            rows.append(",".join(map(str,row)))
    weather=("\r\n".join(headers+rows)+"\r\n").encode("ascii")
    (root/"weather.epw").write_bytes(weather)
    expected={"case":case,"zones":zones,"days":days,"zone_timesteps_per_hour":6,"expected_intervals":days*144,"native_execution":False,"weather_is_measured":False,"original_license":"Apache-2.0","native_limits":{"timeout_seconds":900 if case!="resource8" else 1800,"memory_bytes":2147483648,"scratch_bytes":1073741824,"file_bytes":134217728},"archive_limits":{"entries":5000,"entry_bytes":67108864,"total_bytes":268435456},"input_sha256":{"model.epJSON":hashlib.sha256(model_bytes).hexdigest(),"weather.epw":hashlib.sha256(weather).hexdigest()},"generator_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    if case=="electrical":expected["oracle"]={"equipment_kwh":4380,"interior_lights_kwh":2190,"exterior_lights_kwh":438,"facility_kwh":7008,"energy_band":"max(0.01 kWh,0.01% reference)","every_interval_facility_j":480000}
    if case=="envelope":expected["oracle"]={"formula":"12 m2*(35-25) C/(2+.1+.04) m2K/W","heat_w":120/2.14,"band_w":1,"steady_state":"final144 intervals max-min <=0.01 W","excluded":"solar, gains, ventilation, infiltration, other exterior surfaces; IdealLoads diagnostic only","longwave":"schema requires positive emissivity; 1e-9 gives <1e-3 W omitted-radiation bound for all94m2 surfaces below400K"}
    if case in ("dx","resource8"):expected["oracle"]={"balanced_ventilation_m3_s_per_zone":.02,"balanced_fan_w_per_zone":4,"ventilation_always_on":True,"ac_availability":"06:00-24:00; recirculating only; supply fan continuous while available","off_cycle_check":"coil and supply fan zero; ventilation .02m3/s and4W remains","meter_reconciliation":"max(.01kWh,.01%) energy; max(.001kW,.01%) demand","fixed_curves":"original synthetic constant capacity/EIR modifiers; PLF=.85+.15PLR; not manufacturer performance"}
    (root/"fixture-declaration.json").write_text(json.dumps(expected,indent=2)+"\n",encoding="utf8")
    return expected


if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--case",choices=CASES,required=True);p.add_argument("--output",type=Path,required=True)
    a=p.parse_args();print(json.dumps(make(a.case,a.output)))
