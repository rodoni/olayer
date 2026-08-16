use crate::aeronautical::aixm_parser::parse_aixm_51_str;
use crate::aeronautical::geojson_parser::{export_dataset_to_geojson, parse_geojson_aviation_str};
use crate::aeronautical::types::{AirspaceType, NavaidType};
use crate::geodesy::coords::LatLon;

const SAMPLE_AIXM_51_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<aixm:AIXMBasicMessage xmlns:aixm="http://www.aixm.aero/schema/5.1"
                       xmlns:gml="http://www.opengis.net/gml/3.2"
                       xmlns:xlink="http://www.w3.org/1999/xlink">
    <aixm:hasMember>
        <aixm:Airspace gml:id="LFFF_TMA_PARIS">
            <aixm:identifier>LFFF_TMA_PARIS</aixm:identifier>
            <aixm:name>PARIS TMA SECTOR 1</aixm:name>
            <aixm:type>TMA</aixm:type>
            <aixm:AirspaceVolume>
                <aixm:upperLimit uom="FL">195</aixm:upperLimit>
                <aixm:upperLimitReference>STD</aixm:upperLimitReference>
                <aixm:lowerLimit uom="FT">2500</aixm:lowerLimit>
                <aixm:lowerLimitReference>MSL</aixm:lowerLimitReference>
            </aixm:AirspaceVolume>
            <gml:Polygon gml:id="POLY_1">
                <gml:exterior>
                    <gml:LinearRing>
                        <gml:posList>49.0 2.0 49.5 2.5 49.2 3.0 48.8 2.8 49.0 2.0</gml:posList>
                    </gml:LinearRing>
                </gml:exterior>
            </gml:Polygon>
        </aixm:Airspace>
    </aixm:hasMember>
    <aixm:hasMember>
        <aixm:Navaid gml:id="NAV_EPM">
            <aixm:designator>EPM</aixm:designator>
            <aixm:name>EPSOM VOR-DME</aixm:name>
            <aixm:type>VOR_DME</aixm:type>
            <aixm:frequency>115.65</aixm:frequency>
            <aixm:elevation>150.0</aixm:elevation>
            <gml:Point gml:id="PT_EPM">
                <gml:pos>51.332 -0.370</gml:pos>
            </gml:Point>
        </aixm:Navaid>
    </aixm:hasMember>
</aixm:AIXMBasicMessage>
"#;

const SAMPLE_GEOJSON_AVIATION: &str = r#"{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "properties": {
        "aero_type": "Airspace",
        "uid": "EGLL_CTR",
        "name": "HEATHROW CTR",
        "airspace_type": "CTR",
        "lower_limit_m": 0.0,
        "upper_limit_fl": 60
      },
      "geometry": {
        "type": "Polygon",
        "coordinates": [
          [
            [-0.6, 51.4, 0.0],
            [-0.3, 51.6, 0.0],
            [-0.2, 51.4, 0.0],
            [-0.5, 51.3, 0.0],
            [-0.6, 51.4, 0.0]
          ]
        ]
      }
    },
    {
      "type": "Feature",
      "properties": {
        "aero_type": "Navaid",
        "ident": "LON",
        "name": "LONDON VOR",
        "navaid_type": "VOR",
        "frequency_mhz": 113.6,
        "elevation_m": 25.0
      },
      "geometry": {
        "type": "Point",
        "coordinates": [-0.46, 51.47, 25.0]
      }
    },
    {
      "type": "Feature",
      "properties": {
        "aero_type": "Airport",
        "icao": "EGLL",
        "iata": "LHR",
        "name": "London Heathrow Airport",
        "elevation_m": 25.0
      },
      "geometry": {
        "type": "Point",
        "coordinates": [-0.4614, 51.4700, 25.0]
      }
    }
  ]
}"#;

#[test]
fn test_parse_aixm_51_basic() {
    let dataset = parse_aixm_51_str(SAMPLE_AIXM_51_XML).expect("Failed to parse AIXM 5.1 XML");
    assert_eq!(dataset.airspaces.len(), 1);
    assert_eq!(dataset.navaids.len(), 1);

    let asp = &dataset.airspaces[0];
    assert_eq!(asp.uid, "LFFF_TMA_PARIS");
    assert_eq!(asp.airspace_type, AirspaceType::Tma);
    assert_eq!(asp.upper_limit.flight_level, Some(195));
    assert_eq!(asp.boundary.len(), 5);

    let nav = &dataset.navaids[0];
    assert_eq!(nav.ident, "EPM");
    assert_eq!(nav.navaid_type, NavaidType::VorDme);
    assert_eq!(nav.frequency_mhz, Some(115.65));
    assert_eq!(nav.elevation_m, Some(150.0));
    assert!((nav.coords.lat.to_degrees() - 51.332).abs() < 1e-5);
    assert!((nav.coords.lon.to_degrees() - (-0.370)).abs() < 1e-5);
}

#[test]
fn test_parse_geojson_aviation_and_roundtrip() {
    let dataset = parse_geojson_aviation_str(SAMPLE_GEOJSON_AVIATION)
        .expect("Failed to parse GeoJSON Aviation");

    assert_eq!(dataset.airspaces.len(), 1);
    assert_eq!(dataset.navaids.len(), 1);
    assert_eq!(dataset.airports.len(), 1);

    let airspace = &dataset.airspaces[0];
    assert_eq!(airspace.uid, "EGLL_CTR");
    assert_eq!(airspace.airspace_type, AirspaceType::Ctr);
    assert_eq!(airspace.upper_limit.flight_level, Some(60));

    let navaid = dataset.find_navaid("LON").expect("LON navaid should be found");
    assert_eq!(navaid.navaid_type, NavaidType::Vor);
    assert_eq!(navaid.frequency_mhz, Some(113.6));

    let airport = dataset.find_airport("EGLL").expect("EGLL airport should be found");
    assert_eq!(airport.iata.as_deref(), Some("LHR"));

    // Roundtrip back to GeoJSON
    let exported = export_dataset_to_geojson(&dataset).expect("Export to GeoJSON failed");
    let re_parsed = parse_geojson_aviation_str(&exported).expect("Re-parse GeoJSON failed");

    assert_eq!(re_parsed.airspaces.len(), 1);
    assert_eq!(re_parsed.navaids.len(), 1);
    assert_eq!(re_parsed.airports.len(), 1);
}

#[test]
fn test_spatial_queries_in_dataset() {
    let dataset = parse_geojson_aviation_str(SAMPLE_GEOJSON_AVIATION).unwrap();

    // Heathrow coordinates: 51.47, -0.46
    let lhr = LatLon::from_degrees(51.47, -0.46, 25.0);

    // Navaids within 50 km of Heathrow
    let nearby_navaids = dataset.find_navaids_within_radius(&lhr, 50_000.0);
    assert_eq!(nearby_navaids.len(), 1);
    assert_eq!(nearby_navaids[0].ident, "LON");

    // Navaids within 10 meters of a distant point (Paris)
    let paris = LatLon::from_degrees(48.8566, 2.3522, 50.0);
    let empty_navaids = dataset.find_navaids_within_radius(&paris, 100.0);
    assert!(empty_navaids.is_empty());

    // Containment query: point inside Heathrow CTR
    let inside_ctr = LatLon::from_degrees(51.45, -0.4, 0.0);
    let containing = dataset.find_airspaces_containing_point(&inside_ctr);
    assert_eq!(containing.len(), 1);
    assert_eq!(containing[0].uid, "EGLL_CTR");
}
