#[allow(dead_code)] // Retained for the material-property portion of future FGD output.
pub static MATERIAL_BASE_DEF: &str = r#"@BaseClass = material
[
    Material_Emissive_color(color) : "Color emitted by the brush" : "1.0 0 0"
    Material_Ambient_color(color) : "Ambient color of the brush" : "0 1.0 0"
    Material_Diffuse_color(color) : "Diffuse color of the brush" : "0 0 1.0"

    Material_Alpha(float) : "Material Transparency" : "1.0"
	Material_Alpha_UseBlend(choices) : "Use alpha blending for this brush" : 0 =
    [
        0 : "False"
        1 : "True"
    ]
    Material_Alpha_BlendSourceMode(choices) : "Mode to use for alpha blending on the light source": 0 =
    [
        0 : "One"
        2 : "Zero"
        4 : "Source Color"
        6 : "One Minus Source Color"
        8 : "Destination Color"
        10 : "One Minus Destination Color"
        12 : "Source Alpha"
        14 : "One Minus Source Alpha"
        16 : "Destination Alpha"
        18 : "One Minus Destination Alpha"
        20 : "Source Alpha Saturate"
    ]
    Material_Alpha_BlendDestinationMode(choices) : "Mode to use for alpha blending on the light destination": 0 =
    [
        0 : "One"
        32 : "Zero"
        64 : "Source Color"
        96 : "One Minus Source Color"
        128 : "Destination Color"
        160 : "One Minus Destination Color"
        192 : "Source Alpha"
        224 : "One Minus Source Alpha"
        256 : "Destination Alpha"
        288 : "One Minus Destination Alpha"
        320 : "Source Alpha Saturate"
    ]

	Material_Alpha_TestEnable(choices) : "Use alpha testing for this brush" : 0 =
    [
        0 : "False"
        512 : "True"
    ]
	Material_Alpha_TestFunction(choices) : "Use alpha testing for this brush" : 0 =
    [
        0 : "Always"
        1024 : "Less"
        2048 : "Equal"
        3072 : "Less Than Or Equal"
        4096 : "Greater Than"
        5120 : "Not Equal"
        6144 : "Greater Than Or Equal"
        7168 : "Never"
    ]
    Material_Alpha_TestThreshold(float) : "Threshold to use against the background when alpha testing": "1.0"

	Material_Alpha_NoSort(choices) : "Disable triangle sorting for this object" : 0 =
    [
        0 : "False"
        8192 : "True"
    ]
]"#;
