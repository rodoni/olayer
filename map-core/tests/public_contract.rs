use olayer_map_core::{
    BoundingBox, Capabilities, CoordinateReferenceSystem, QueryParameter, ServiceType, UrlParts,
    WcsRequestBuilder, WfsRequestBuilder, WmsRequestBuilder, INVALID_CAPABILITIES, INVALID_WCS,
    INVALID_WFS,
};
use std::num::NonZeroU32;

#[test]
fn urls_preserve_existing_queries_and_fragments_and_encode_utf8() {
    let params = [QueryParameter::new("a &", "é !'()*~/%")];
    for (base, path, expected) in [
        ("https://example.test/ows?token=raw%20value#view", "", "https://example.test/ows?token=raw%20value&a%20%26=%C3%A9%20%21%27%28%29%2A~%2F%25#view"),
        ("https://example.test/", "ows?token=1#view", "https://example.test/ows?token=1&a%20%26=%C3%A9%20%21%27%28%29%2A~%2F%25#view"),
        ("https://example.test?token=1#old", "ows?extra=2#new", "https://example.test/ows?token=1&extra=2&a%20%26=%C3%A9%20%21%27%28%29%2A~%2F%25#new"),
    ] {
        let endpoint = UrlParts::new(base, path);
        assert_eq!(endpoint.url(&params), expected);
        assert_eq!(endpoint.url(&params), endpoint.url(&params));
    }
    for endpoint in ["https://example.test/ows?", "https://example.test/ows?a=1&"] {
        assert_eq!(
            UrlParts::new(endpoint, "").url(&[QueryParameter::new("x", "1")]),
            format!("{endpoint}x=1")
        );
    }
    assert_eq!(
        UrlParts::new("https://example.test/ows?x=1#f", "").url(&[]),
        "https://example.test/ows?x=1#f"
    );
}

#[test]
fn wms_13_changes_only_wire_axis_order() {
    for (epsg, wire_bounds) in [(4326, "2%2C1%2C4%2C3"), (3857, "1%2C2%2C3%2C4")] {
        let bounds = BoundingBox::new(
            1.0,
            2.0,
            3.0,
            4.0,
            CoordinateReferenceSystem::epsg(epsg).unwrap(),
        )
        .unwrap();
        let request = WmsRequestBuilder::new(
            UrlParts::new("https://example.test/ows?key=1#f", ""),
            "roads",
        )
        .bounds(bounds)
        .size(NonZeroU32::new(2).unwrap(), NonZeroU32::new(3).unwrap())
        .get_map()
        .unwrap();
        let url = request.url();
        assert!(url.contains("VERSION=1.3.0"));
        assert!(url.contains(&format!("BBOX={wire_bounds}")));
        assert!(url.starts_with("https://example.test/ows?key=1&SERVICE=WMS"));
        assert!(url.ends_with("#f"));
        assert_eq!(request.bounds, bounds);
    }
}

#[test]
fn builders_reject_conflicting_coverage_crs_and_zero_feature_count() {
    let geographic = CoordinateReferenceSystem::epsg(4326).unwrap();
    let projected = CoordinateReferenceSystem::epsg(3857).unwrap();
    let bounds = BoundingBox::new(1.0, 2.0, 3.0, 4.0, geographic).unwrap();
    let coverage = WcsRequestBuilder::new(UrlParts::new("https://example.test", "ows"), "height")
        .crs(projected)
        .bounds(bounds)
        .size(2, 2)
        .get_coverage();
    assert_eq!(coverage.unwrap_err().code, INVALID_WCS);
    for count in [0, 1, u32::MAX] {
        let result = WfsRequestBuilder::new(UrlParts::new("https://example.test", "ows"), "roads")
            .crs(geographic)
            .count(count)
            .get_feature();
        if count == 0 {
            assert_eq!(result.unwrap_err().code, INVALID_WFS);
        } else {
            assert_eq!(result.unwrap().count, Some(count));
        }
    }
}

#[test]
fn capabilities_identify_legacy_wms_and_collect_wcs_identifiers() {
    let legacy = Capabilities::parse_xml(br#"<WMT_MS_Capabilities version="1.1.1"><Service><Title>Legacy</Title></Service><Layer><Name>roads</Name></Layer></WMT_MS_Capabilities>"#).unwrap();
    assert_eq!(legacy.service, ServiceType::Wms);
    assert_eq!(legacy.layers, ["roads"]);
    let wcs = Capabilities::parse_xml(br#"<wcs:WCS_Capabilities xmlns:wcs="urn:wcs" version="2.0.1"><Title>Cov<![CDATA[er]]>ages</Title><Contents><CoverageSummary><wcs:CoverageId>height</wcs:CoverageId></CoverageSummary><CoverageSummary><Identifier>legacy</Identifier></CoverageSummary></Contents></wcs:WCS_Capabilities>"#).unwrap();
    assert_eq!(wcs.service, ServiceType::Wcs);
    assert_eq!(wcs.title, "Coverages");
    assert_eq!(wcs.layers, ["height", "legacy"]);
}

fn assert_invalid(xml: &[u8]) {
    match Capabilities::parse_xml(xml) {
        Err(error) => assert_eq!(error.code, INVALID_CAPABILITIES),
        Ok(summary) => panic!(
            "accepted invalid XML {:?}: {summary:?}",
            String::from_utf8_lossy(xml)
        ),
    }
}

#[test]
fn capabilities_reject_malformed_truncated_multiple_roots_and_dtds() {
    for xml in [
        "",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><Layer>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Wrong></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities><extra/>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities><WMS_Capabilities/>",
        "text<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities>text",
        "<!DOCTYPE WMS_Capabilities><WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities>",
        "<!DOCTYPE WMS_Capabilities [<!ENTITY x 'Maps'>]><WMS_Capabilities version=\"1.3.0\"><Title>&x;</Title></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\" version=\"2\"><Title>Maps</Title></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><Layer broken/></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities><",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities><!--",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title></WMS_Capabilities><?broken",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><![CDATA[truncated",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><!-- invalid -- comment --></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><Layer attr=\"&unknown;\"/></WMS_Capabilities>",
        "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title><?xml version=\"1.0\"?></WMS_Capabilities>",
    ] {
        assert_invalid(xml.as_bytes());
    }
}

#[test]
fn capabilities_accept_utf8_declarations_comments_and_split_identifier_text() {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
        <!-- summary -->
        <Capabilities version="1.0.0"><Title>Tiles &amp; Maps</Title>
        <Layer><Identifier>ro<!-- split -->a<![CDATA[ds]]></Identifier></Layer>
        </Capabilities><!-- trailing comment -->"#;
    let summary = Capabilities::parse_xml(xml).unwrap();
    assert_eq!(summary.service, ServiceType::Wmts);
    assert_eq!(summary.title, "Tiles & Maps");
    assert_eq!(summary.layers, ["roads"]);
    assert_invalid(b"\xff");
    assert_invalid(br#"<?xml version="1.0" encoding="UTF-16"?><Capabilities version="1"><Title>Maps</Title></Capabilities>"#);
}

#[test]
fn capabilities_reject_every_truncation_of_a_complete_document() {
    let xml = br#"<WMS_Capabilities version="1.3.0"><Title>Maps &amp; roads</Title><Layer><Name>roads</Name></Layer></WMS_Capabilities>"#;
    for end in 0..xml.len() {
        assert_invalid(&xml[..end]);
    }
    assert!(Capabilities::parse_xml(xml).is_ok());
}

#[test]
fn capabilities_validate_attribute_heavy_elements_without_skipping_duplicates() {
    let attributes: String = (0..4096).map(|index| format!(" a{index}=\"v\"")).collect();
    let valid =
        format!("<Capabilities version=\"1\"{attributes}><Title>Maps</Title></Capabilities>");
    assert!(Capabilities::parse_xml(valid.as_bytes()).is_ok());
    let duplicate = format!("<Capabilities version=\"1\"{attributes} a4095=\"duplicate\"><Title>Maps</Title></Capabilities>");
    assert_invalid(duplicate.as_bytes());
}

#[test]
fn capabilities_enforce_exact_resource_boundaries() {
    let head = "<WMS_Capabilities version=\"1.3.0\"><Title>Maps</Title>";
    let tail = "</WMS_Capabilities>";
    let size_limit = 1024 * 1024;
    let at_limit = format!(
        "{head}{}{tail}",
        " ".repeat(size_limit - head.len() - tail.len())
    );
    assert_eq!(at_limit.len(), size_limit);
    assert!(Capabilities::parse_xml(at_limit.as_bytes()).is_ok());
    assert_invalid(format!("{at_limit} ").as_bytes());

    for (nested, valid) in [(63, true), (64, false)] {
        let xml = format!(
            "{head}{}{}{tail}",
            "<Layer>".repeat(nested),
            "</Layer>".repeat(nested)
        );
        if valid {
            assert!(Capabilities::parse_xml(xml.as_bytes()).is_ok());
        } else {
            assert_invalid(xml.as_bytes());
        }
    }
    for (count, valid) in [(4096, true), (4097, false)] {
        let xml = format!(
            "{head}{}{tail}",
            "<Layer><Name>roads</Name></Layer>".repeat(count)
        );
        if valid {
            assert_eq!(
                Capabilities::parse_xml(xml.as_bytes())
                    .unwrap()
                    .layers
                    .len(),
                count
            );
        } else {
            assert_invalid(xml.as_bytes());
        }
    }
}
