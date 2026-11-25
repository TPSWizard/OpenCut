//! Mastering and the meter against `fixtures/loudness/cases.json`, written
//! by an independent reference. The signals are rebuilt from their recipe
//! (quantised to 16 bit, as the reference does).

use loudness::{Meter, master};
use serde_json::Value;

/// A recipe's channels: sines, clicks and seeded noise, summed and quantised.
pub fn signal(recipe: &Value) -> (u32, Vec<Vec<f32>>) {
    let rate = recipe["rate"].as_u64().unwrap() as u32;
    let frames = recipe["frames"].as_u64().unwrap() as usize;
    let channels = recipe["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|components| {
            let mut channel = vec![0.0f64; frames];
            for component in components.as_array().unwrap() {
                if let Some(sine) = component.get("sine") {
                    let (frequency, amplitude) = (sine["freq"].as_f64().unwrap(), sine["amp"].as_f64().unwrap());
                    let phase = sine.get("phase").and_then(Value::as_f64).unwrap_or(0.0);
                    let from = sine.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let to = (sine.get("to").and_then(Value::as_u64).unwrap_or(frames as u64) as usize).min(frames);
                    for (index, value) in channel.iter_mut().enumerate().take(to).skip(from) {
                        *value += amplitude * (2.0 * std::f64::consts::PI * frequency * index as f64 / rate as f64 + phase).sin();
                    }
                }
                if let Some(clicks) = component.get("clicks") {
                    for click in clicks.as_array().unwrap() {
                        channel[click[0].as_u64().unwrap() as usize] += click[1].as_f64().unwrap();
                    }
                }
                if let Some(noise) = component.get("noise") {
                    let (mut state, amplitude) = (noise["seed"].as_u64().unwrap(), noise["amp"].as_f64().unwrap());
                    for value in channel.iter_mut() {
                        state = (state * 1_103_515_245 + 12_345) % 2_147_483_648;
                        *value += amplitude * (state as f64 / 2_147_483_648.0 * 2.0 - 1.0);
                    }
                }
            }
            channel.iter().map(|value| ((value * 32767.0).round() / 32767.0).clamp(-1.0, 1.0) as f32).collect()
        })
        .collect();
    (rate, channels)
}

fn close(got: f64, want: &Value, tolerance: f64, what: &str) {
    match want.as_f64() {
        Some(want) => assert!((got - want).abs() <= tolerance, "{what}: {got} vs {want}"),
        None => assert!(!got.is_finite(), "{what}: {got} should be -inf"),
    }
}

#[test]
fn matches_the_reference() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/loudness/cases.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 8);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (rate, mut channels) = signal(&case["signal"]);
        match case["kind"].as_str().unwrap() {
            "master" => {
                let report = master(&mut channels, rate, case["targetLufs"].as_f64().unwrap(), case["ceilingDbtp"].as_f64().unwrap());
                let want = &case["report"];
                close(report.input_lufs, &want["inputLufs"], 1e-6, name);
                close(report.input_true_peak_dbtp, &want["inputTruePeakDbtp"], 1e-6, name);
                close(report.gain_db, &want["gainDb"], 1e-6, name);
                close(report.output_lufs, &want["outputLufs"], 1e-4, name);
                close(report.output_true_peak_dbtp, &want["outputTruePeakDbtp"], 1e-4, name);
                assert_eq!(report.limited, want["limited"].as_bool().unwrap(), "{name}");
                assert_eq!(report.passes as u64, want["passes"].as_u64().unwrap(), "{name}");
                if report.limited {
                    assert!(report.output_true_peak_dbtp <= case["ceilingDbtp"].as_f64().unwrap(), "{name}: over the ceiling");
                }
                for (channel, (sum, samples)) in
                    channels.iter().zip(case["sumAbs"].as_array().unwrap().iter().zip(case["samples"].as_array().unwrap()))
                {
                    let got: f64 = channel.iter().map(|sample| (*sample as f64).abs()).sum();
                    assert!((got - sum.as_f64().unwrap()).abs() < 1e-3, "{name}: sum {got} vs {sum}");
                    for (pick, want) in case["picks"].as_array().unwrap().iter().zip(samples.as_array().unwrap()) {
                        let got = channel[pick.as_u64().unwrap() as usize] as f64;
                        assert!((got - want.as_f64().unwrap()).abs() < 1e-6, "{name}: sample {pick}: {got} vs {want}");
                    }
                }
            }
            "meter" => {
                let chunks: Vec<usize> = case["chunks"].as_array().unwrap().iter().map(|size| size.as_u64().unwrap() as usize).collect();
                let mut meter = Meter::new(rate, channels.len());
                let length = channels[0].len();
                let (mut at, mut chunk) = (0, 0);
                for want in case["readings"].as_array().unwrap() {
                    let size = chunks[chunk % chunks.len()];
                    chunk += 1;
                    let end = (at + size).min(length);
                    let planes: Vec<&[f32]> = channels.iter().map(|channel| &channel[at..end]).collect();
                    meter.push_planar(&planes);
                    at = end;
                    assert_eq!(at as u64, want["frames"].as_u64().unwrap());
                    let readings = meter.readings();
                    close(readings.momentary_lufs, &want["momentaryLufs"], 1e-6, name);
                    close(readings.short_term_lufs, &want["shortTermLufs"], 1e-6, name);
                    close(readings.peak_dbfs, &want["peakDbfs"], 1e-6, name);
                }
                // Interleaved input reads the same.
                let mut interleaved = Meter::new(rate, channels.len());
                let samples: Vec<f32> = (0..length).flat_map(|index| channels.iter().map(move |channel| channel[index])).collect();
                interleaved.push_interleaved(&samples);
                assert_eq!(interleaved.readings(), meter.readings(), "{name}");
            }
            other => panic!("unknown kind {other}"),
        }
    }
}
