use olayer_map_core::interop::build_request_json;
use serde_json::Value;

#[test]
fn shared_versioned_wms_fixture_has_canonical_wire_url() {
    let request = include_str!("../../tests/fixtures/map-core/wms-4326.json");
    let result: Value = serde_json::from_str(&build_request_json(request).unwrap()).unwrap();
    assert_eq!(result["version"], 1);
    assert_eq!(result["url"], "https://example.test/ows?tenant=one&SERVICE=WMS&REQUEST=GetMap&VERSION=1.3.0&LAYERS=roads&STYLES=&CRS=EPSG%3A4326&BBOX=20%2C10%2C40%2C30&WIDTH=256&HEIGHT=128&FORMAT=image%2Fpng&TRANSPARENT=true#map");
}

#[test]
fn rejects_wrong_wire_version_and_zero_dimensions() {
    let wrong_version = r#"{"version":2,"operation":{"kind":"get_capabilities","endpoint":{"base":"https://example.test","path":""},"service":"WMS"}}"#;
    assert_eq!(
        build_request_json(wrong_version).unwrap_err().code.as_str(),
        "OL-CORE-MAP-0012"
    );
    let invalid_dimensions = r#"{"version":1,"operation":{"kind":"wms","endpoint":{"base":"https://example.test","path":""},"layers":"roads","styles":"","bounds":{"epsg":4326,"min_x":0,"min_y":0,"max_x":1,"max_y":1},"width":0,"height":1,"format":"image/png","transparency":"opaque"}}"#;
    assert_eq!(
        build_request_json(invalid_dimensions)
            .unwrap_err()
            .code
            .as_str(),
        "OL-CORE-MAP-0006"
    );
}

#[test]
fn serializes_each_service_through_its_typed_builder() {
    let cases = [
        (
            r#"{"version":1,"operation":{"kind":"wmts","endpoint":{"base":"https://example.test","path":"wmts"},"layer":"roads","style":"default","matrix_set":"custom","matrix":{"identifier":"matrix:3","epsg":3857,"matrix_width":2,"matrix_height":3,"tile_width":512,"tile_height":256},"tile":{"x":1,"y":2,"z":3},"format":"image/png"}}"#,
            "TILEMATRIX=matrix%3A3&TILEROW=2&TILECOL=1",
        ),
        (
            r#"{"version":1,"operation":{"kind":"wfs","endpoint":{"base":"https://example.test","path":"wfs"},"type_name":"roads","epsg":4326,"format":"application/geo+json","count":10}}"#,
            "TYPENAMES=roads&SRSNAME=EPSG%3A4326&OUTPUTFORMAT=application%2Fgeo%2Bjson&COUNT=10",
        ),
        (
            r#"{"version":1,"operation":{"kind":"wcs","endpoint":{"base":"https://example.test","path":"wcs"},"coverage_id":"elevation","epsg":4326,"bounds":{"epsg":4326,"min_x":0,"min_y":1,"max_x":2,"max_y":3},"width":2,"height":1,"format":"image/tiff"}}"#,
            "COVERAGEID=elevation&SUBSET=x%280%2C2%29&SUBSET=y%281%2C3%29",
        ),
        (
            r#"{"version":1,"operation":{"kind":"get_capabilities","endpoint":{"base":"https://example.test","path":"ows"},"service":"WMTS"}}"#,
            "SERVICE=WMTS&REQUEST=GetCapabilities",
        ),
    ];
    for (request, fragment) in cases {
        let value: Value = serde_json::from_str(&build_request_json(request).unwrap()).unwrap();
        assert!(value["url"].as_str().unwrap().contains(fragment));
    }
}
