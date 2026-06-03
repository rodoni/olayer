use crate::sld::errors::SldError;
use crate::sld::parser::parse;
use crate::sld::styles::{FillStyle, PointStyle, StrokeStyle, TextStyle};

#[test]
fn test_parse_full_sld() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0" xmlns="http://www.opengis.net/sld">
  <NamedLayer>
    <Name>Aerovias</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <Name>RegraAerovia</Name>
          <MinScaleDenominator>5000</MinScaleDenominator>
          <MaxScaleDenominator>100000</MaxScaleDenominator>
          <LineSymbolizer>
            <Stroke>
              <CssParameter name="stroke">#FF5500</CssParameter>
              <CssParameter name="stroke-width">2.5</CssParameter>
              <CssParameter name="stroke-dasharray">5, 2, 1, 2</CssParameter>
            </Stroke>
          </LineSymbolizer>
          <PolygonSymbolizer>
            <Fill>
              <CssParameter name="fill">#00FF00</CssParameter>
              <CssParameter name="fill-opacity">0.8</CssParameter>
            </Fill>
          </PolygonSymbolizer>
          <TextSymbolizer>
            <Label>
              <PropertyName>identificador</PropertyName>
            </Label>
            <Font>
              <CssParameter name="font-family">Roboto</CssParameter>
              <CssParameter name="font-size">14</CssParameter>
            </Font>
            <Fill>
              <CssParameter name="fill">#333333</CssParameter>
            </Fill>
          </TextSymbolizer>
          <PointSymbolizer>
            <Graphic>
              <Mark>
                <WellKnownName>triangle</WellKnownName>
                <Fill>
                  <CssParameter name="fill">#FF0000</CssParameter>
                </Fill>
                <Stroke>
                  <CssParameter name="stroke">#FFFFFF</CssParameter>
                  <CssParameter name="stroke-width">1.0</CssParameter>
                </Stroke>
              </Mark>
              <Size>12.0</Size>
            </Graphic>
          </PointSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;

    let registry = parse(xml).unwrap();
    assert!(registry.layers.contains_key("Aerovias"));
    let rules = &registry.layers["Aerovias"];
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    assert_eq!(rule.name, "RegraAerovia");
    assert_eq!(rule.min_scale, Some(5000.0));
    assert_eq!(rule.max_scale, Some(100000.0));

    assert_eq!(
        rule.stroke,
        Some(StrokeStyle {
            color: "#FF5500".to_string(),
            width: 2.5,
            dash_array: Some(vec![5.0, 2.0, 1.0, 2.0]),
        })
    );

    assert_eq!(
        rule.fill,
        Some(FillStyle {
            color: "#00FF00".to_string(),
            opacity: 0.8,
        })
    );

    assert_eq!(
        rule.text,
        Some(TextStyle {
            label_expression: "identificador".to_string(),
            font_family: "Roboto".to_string(),
            font_size: 14.0,
            fill_color: "#333333".to_string(),
        })
    );

    assert_eq!(
        rule.point,
        Some(PointStyle {
            well_known_name: "triangle".to_string(),
            size: 12.0,
            fill_color: Some("#FF0000".to_string()),
            stroke_color: Some("#FFFFFF".to_string()),
            stroke_width: Some(1.0),
        })
    );
}

#[test]
fn test_applicable_rules_by_scale() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>Setores</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <Name>ZoomClose</Name>
          <MinScaleDenominator>0</MinScaleDenominator>
          <MaxScaleDenominator>5000</MaxScaleDenominator>
          <LineSymbolizer><Stroke><CssParameter name="stroke">#FF0000</CssParameter></Stroke></LineSymbolizer>
        </Rule>
        <Rule>
          <Name>ZoomFar</Name>
          <MinScaleDenominator>5000</MinScaleDenominator>
          <MaxScaleDenominator>50000</MaxScaleDenominator>
          <LineSymbolizer><Stroke><CssParameter name="stroke">#0000FF</CssParameter></Stroke></LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;

    let registry = parse(xml).unwrap();

    // Scale 2000 should match ZoomClose only
    let rules = registry.get_applicable_rules("Setores", 2000.0);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "ZoomClose");

    // Scale 25000 should match ZoomFar only
    let rules = registry.get_applicable_rules("Setores", 25000.0);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "ZoomFar");

    // Scale 100000 should match none
    let rules = registry.get_applicable_rules("Setores", 100000.0);
    assert!(rules.is_empty());
}

#[test]
fn test_xml_malformed() {
    let xml = "<NamedLayer><Name>MissingClose</Name>";
    let res = parse(xml);
    assert!(matches!(res, Err(SldError::XmlError(_))));
}

#[test]
fn test_invalid_numeric_values() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>ErrorLayer</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <MinScaleDenominator>not_a_number</MinScaleDenominator>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let res = parse(xml);
    assert!(matches!(res, Err(SldError::InvalidValue(_))));
}

#[test]
fn test_ignore_namespaces() {
    let xml = r#"
<sld:StyledLayerDescriptor xmlns:sld="http://www.opengis.net/sld" xmlns:se="http://www.opengis.net/se">
  <sld:NamedLayer>
    <sld:Name>NamespaceTest</sld:Name>
    <sld:UserStyle>
      <se:FeatureTypeStyle>
        <se:Rule>
          <se:Name>RuleWithNs</se:Name>
          <se:LineSymbolizer>
            <se:Stroke>
              <se:SvgParameter name="stroke">#00FF00</se:SvgParameter>
              <se:SvgParameter name="stroke-width">3.0</se:SvgParameter>
            </se:Stroke>
          </se:LineSymbolizer>
        </se:Rule>
      </se:FeatureTypeStyle>
    </sld:UserStyle>
  </sld:NamedLayer>
</sld:StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    assert!(registry.layers.contains_key("NamespaceTest"));
    let rules = &registry.layers["NamespaceTest"];
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "RuleWithNs");

    let stroke = rules[0].stroke.as_ref().unwrap();
    assert_eq!(stroke.color, "#00FF00");
    assert_eq!(stroke.width, 3.0);
}

#[test]
fn test_empty_xml() {
    let xml = "";
    let registry = parse(xml).unwrap();
    assert!(registry.layers.is_empty());
}

#[test]
fn test_xml_declaration_only() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>"#;
    let registry = parse(xml).unwrap();
    assert!(registry.layers.is_empty());
}

#[test]
fn test_multiple_named_layers() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>LayerA</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule><Name>A1</Name></Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
  <NamedLayer>
    <Name>LayerB</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule><Name>B1</Name></Rule>
        <Rule><Name>B2</Name></Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    assert_eq!(registry.layers.len(), 2);
    assert_eq!(registry.layers["LayerA"].len(), 1);
    assert_eq!(registry.layers["LayerB"].len(), 2);
}

#[test]
fn test_malformed_dash_array() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>DashTest</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <LineSymbolizer>
            <Stroke>
              <CssParameter name="stroke-dasharray">foo, bar</CssParameter>
            </Stroke>
          </LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let res = parse(xml);
    assert!(matches!(res, Err(SldError::InvalidValue(_))));
}

#[test]
fn test_empty_dash_array_parses_ok() {
    // Whitespace-only text inside a CssParameter is ignored by the parser,
    // so the rule is created without a dash_array value.
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>DashTest</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <LineSymbolizer>
            <Stroke>
              <CssParameter name="stroke-dasharray">   </CssParameter>
            </Stroke>
          </LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    let rule = &registry.layers["DashTest"][0];
    let stroke = rule.stroke.as_ref().unwrap();
    // Empty text was skipped, so dash_array remains the default None
    assert!(stroke.dash_array.is_none());
}

#[test]
fn test_svg_parameter_with_namespace() {
    let xml = r#"
<se:StyledLayerDescriptor xmlns:se="http://www.opengis.net/se">
  <se:NamedLayer>
    <se:Name>SvgParamTest</se:Name>
    <se:UserStyle>
      <se:FeatureTypeStyle>
        <se:Rule>
          <se:PolygonSymbolizer>
            <se:Fill>
              <se:SvgParameter name="fill">#AABBCC</se:SvgParameter>
              <se:SvgParameter name="fill-opacity">0.5</se:SvgParameter>
            </se:Fill>
          </se:PolygonSymbolizer>
        </se:Rule>
      </se:FeatureTypeStyle>
    </se:UserStyle>
  </se:NamedLayer>
</se:StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    let rule = &registry.layers["SvgParamTest"][0];
    let fill = rule.fill.as_ref().unwrap();
    assert_eq!(fill.color, "#AABBCC");
    assert_eq!(fill.opacity, 0.5);
}

#[test]
fn test_scale_boundary_inclusion() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>Boundaries</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <Name>ExactBound</Name>
          <MinScaleDenominator>1000</MinScaleDenominator>
          <MaxScaleDenominator>5000</MaxScaleDenominator>
          <LineSymbolizer><Stroke><CssParameter name="stroke">#00FF00</CssParameter></Stroke></LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();

    // Exactly at min_scale
    let rules = registry.get_applicable_rules("Boundaries", 1000.0);
    assert_eq!(rules.len(), 1);

    // Exactly at max_scale
    let rules = registry.get_applicable_rules("Boundaries", 5000.0);
    assert_eq!(rules.len(), 1);

    // Just below min_scale
    let rules = registry.get_applicable_rules("Boundaries", 999.0);
    assert!(rules.is_empty());

    // Just above max_scale
    let rules = registry.get_applicable_rules("Boundaries", 5001.0);
    assert!(rules.is_empty());
}

#[test]
fn test_missing_layer_returns_empty() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>Existing</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule><Name>R1</Name></Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    let rules = registry.get_applicable_rules("NonExistent", 1000.0);
    assert!(rules.is_empty());
}

#[test]
fn test_default_symbolizer_values() {
    // Symbolizers with no CssParameters should retain their default values.
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>Defaults</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <LineSymbolizer><Stroke></Stroke></LineSymbolizer>
          <PolygonSymbolizer><Fill></Fill></PolygonSymbolizer>
          <TextSymbolizer><Label></Label><Font></Font><Fill></Fill></TextSymbolizer>
          <PointSymbolizer><Graphic><Mark></Mark><Size>8.0</Size></Graphic></PointSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    let rule = &registry.layers["Defaults"][0];

    let stroke = rule.stroke.as_ref().unwrap();
    assert_eq!(stroke.color, "#000000");
    assert_eq!(stroke.width, 1.0);
    assert!(stroke.dash_array.is_none());

    let fill = rule.fill.as_ref().unwrap();
    assert_eq!(fill.color, "#000000");
    assert_eq!(fill.opacity, 1.0);

    let text = rule.text.as_ref().unwrap();
    assert_eq!(text.label_expression, "");
    assert_eq!(text.font_family, "sans-serif");
    assert_eq!(text.font_size, 10.0);
    assert_eq!(text.fill_color, "#000000");

    let point = rule.point.as_ref().unwrap();
    assert_eq!(point.well_known_name, "circle");
    assert_eq!(point.size, 8.0);
    assert!(point.fill_color.is_none());
    assert!(point.stroke_color.is_none());
    assert!(point.stroke_width.is_none());
}

#[test]
fn test_self_closing_rule() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>SelfClose</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule />
        <Rule>
          <Name>Normal</Name>
          <LineSymbolizer><Stroke><CssParameter name="stroke">#FF0000</CssParameter></Stroke></LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let registry = parse(xml).unwrap();
    let rules = &registry.layers["SelfClose"];
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].name, "");
    assert_eq!(rules[1].name, "Normal");
}

#[test]
fn test_invalid_stroke_width() {
    let xml = r#"
<StyledLayerDescriptor>
  <NamedLayer>
    <Name>BadStroke</Name>
    <UserStyle>
      <FeatureTypeStyle>
        <Rule>
          <LineSymbolizer>
            <Stroke>
              <CssParameter name="stroke-width">fat</CssParameter>
            </Stroke>
          </LineSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>
"#;
    let res = parse(xml);
    assert!(matches!(res, Err(SldError::InvalidValue(_))));
}
