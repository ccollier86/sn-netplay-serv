use serde::Deserialize;

const DPAD_MASK: u16 = 0x00f0;
const NEUTRAL_INPUT: [u8; 10] = [0; 10];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PredictorFixture {
    predictor: String,
    input_codec: String,
    vectors: Vec<PredictorVector>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PredictorVector {
    name: String,
    mode: String,
    latest_real_hex: Option<String>,
    previous_predicted_hex: Option<String>,
    expected_hex: String,
}

#[test]
fn canonical_predictor_vectors_follow_the_v1_byte_contract() {
    let fixture: PredictorFixture = serde_json::from_str(include_str!(
        "../../../spec/netplay-v5/fixtures/predictor-vectors.json"
    ))
    .expect("predictor fixture");
    assert_eq!(fixture.predictor, "shadowboy-retropad-predictor-v1");
    assert_eq!(fixture.input_codec, "shadowboy-retropad-v1-le");

    for vector in fixture.vectors {
        let latest_real = vector
            .latest_real_hex
            .as_deref()
            .map(decode_input)
            .unwrap_or(NEUTRAL_INPUT);
        let actual = match vector.mode.as_str() {
            "first" => latest_real,
            "resimulation" => resimulate(
                latest_real,
                decode_input(
                    vector
                        .previous_predicted_hex
                        .as_deref()
                        .expect("resimulation requires prior prediction"),
                ),
            ),
            mode => panic!("unknown predictor mode {mode}"),
        };
        assert_eq!(
            actual,
            decode_input(&vector.expected_hex),
            "{}",
            vector.name
        );
    }
}

fn resimulate(latest_real: [u8; 10], previous_predicted: [u8; 10]) -> [u8; 10] {
    let latest_buttons = u16::from_le_bytes([latest_real[0], latest_real[1]]);
    let previous_buttons = u16::from_le_bytes([previous_predicted[0], previous_predicted[1]]);
    let resolved_buttons = (latest_buttons & !DPAD_MASK) | (previous_buttons & DPAD_MASK);
    let mut resolved = previous_predicted;
    resolved[0..2].copy_from_slice(&resolved_buttons.to_le_bytes());
    resolved
}

fn decode_input(value: &str) -> [u8; 10] {
    let bytes = (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).expect("fixture hex"))
        .collect::<Vec<_>>();
    bytes.try_into().expect("Retropad payload is ten bytes")
}
