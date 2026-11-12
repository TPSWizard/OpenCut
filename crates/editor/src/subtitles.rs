//! Writing a project's captions as subtitle files: SubRip (`.srt`), WebVTT
//! (`.vtt`, inline timestamps for word timing) and Advanced SubStation
//! Alpha (`.ass`, `\k` karaoke tags with one style per look). The web
//! app's `subtitles/write.ts` writes the same bytes for the same captions.

use std::path::Path;

use crate::document::{CanvasSize, ElementKind, Project, TrackKind};

/// The web app sizes text as `fontSize` times the canvas height over this.
pub const FONT_SIZE_SCALE_REFERENCE: f64 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
    Ass,
}

impl SubtitleFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
        }
    }

    /// The format a file name's extension asks for (`srt`, `vtt`, `ass`).
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "vtt" => Some(Self::Vtt),
            "ass" | "ssa" => Some(Self::Ass),
            _ => None,
        }
    }
}

/// One word of a cue, in seconds on the timeline clock.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// The parts of a text element's look that a subtitle style line can carry.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportCueStyle {
    pub font_family: String,
    /// In app units (the same space as the `fontSize` parameter): pixels are this times the canvas height over [`FONT_SIZE_SCALE_REFERENCE`].
    pub font_size: f64,
    pub color: String,
    pub bold: bool,
    pub italic: bool,
    /// `left`, `center` or `right`.
    pub text_align: String,
    /// Offset of the text's centre from the canvas centre, in canvas pixels (y down).
    pub position_y: f64,
}

/// A caption ready to be written: times in seconds on the timeline clock.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportCue {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// Word timings when the caption carries them.
    pub words: Vec<ExportWord>,
    pub style: ExportCueStyle,
}

/// The caption cues of the project (visible text elements on visible text
/// tracks, in start order), optionally only those of one track.
pub fn collect_caption_cues(project: &Project, track_id: Option<&str>) -> Vec<ExportCue> {
    let mut cues = Vec::new();
    for track in &project.tracks {
        if track.kind != TrackKind::Text || track.hidden {
            continue;
        }
        if track_id.is_some_and(|id| id != track.id) {
            continue;
        }
        for element in &track.elements {
            if element.kind != ElementKind::Text || element.hidden {
                continue;
            }
            let content = element.params.text("content", "").trim().to_owned();
            if content.is_empty() {
                continue;
            }
            let start = element.start_time.to_seconds_f64();
            let end = start + element.duration.to_seconds_f64();
            let params = &element.params;
            let weight = params.get("fontWeight");
            let bold = weight.is_some_and(|value| {
                value.as_text() == "bold" || value.as_number().is_some_and(|number| number >= 600.0)
            });
            let align = params.text("textAlign", "center").into_owned();
            cues.push(ExportCue {
                text: content,
                start,
                end,
                words: element
                    .words
                    .iter()
                    .map(|word| ExportWord {
                        text: word.text.clone(),
                        start: start + word.start.to_seconds_f64(),
                        end: start + word.end.to_seconds_f64(),
                    })
                    .collect(),
                style: ExportCueStyle {
                    font_family: params.text("fontFamily", "Arial").into_owned(),
                    font_size: params.number("fontSize", 15.0),
                    color: params.text("color", "#ffffff").into_owned(),
                    bold,
                    italic: params.text("fontStyle", "normal") == "italic",
                    text_align: if align == "left" || align == "right" { align } else { "center".to_owned() },
                    position_y: params.number("transform.positionY", 0.0),
                },
            });
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
    cues
}

/// The project's captions as a subtitle file's text.
pub fn write_subtitles(project: &Project, format: SubtitleFormat, track_id: Option<&str>) -> String {
    let cues = collect_caption_cues(project, track_id);
    match format {
        SubtitleFormat::Srt => write_srt(&cues),
        SubtitleFormat::Vtt => write_vtt(&cues),
        SubtitleFormat::Ass => write_ass(&cues, project.settings.canvas, &project.name),
    }
}

/// Writes the project's captions to `path` in the format its extension
/// names (`.srt` when it names none). Returns how many cues were written.
pub fn export_subtitles(project: &Project, path: &Path, track_id: Option<&str>) -> Result<usize, String> {
    let format = SubtitleFormat::from_path(path).unwrap_or(SubtitleFormat::Srt);
    let cues = collect_caption_cues(project, track_id);
    let text = match format {
        SubtitleFormat::Srt => write_srt(&cues),
        SubtitleFormat::Vtt => write_vtt(&cues),
        SubtitleFormat::Ass => write_ass(&cues, project.settings.canvas, &project.name),
    };
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(cues.len())
}

/// `HH:MM:SS` plus milliseconds behind `separator`.
fn clock_time(seconds: f64, separator: char) -> String {
    let total = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total % 1000;
    let s = (total / 1000) % 60;
    let m = (total / 60_000) % 60;
    let h = total / 3_600_000;
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

fn normalise_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn write_srt(cues: &[ExportCue]) -> String {
    let blocks: Vec<String> = cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            format!(
                "{}\n{} --> {}\n{}",
                index + 1,
                clock_time(cue.start, ','),
                clock_time(cue.end, ','),
                normalise_newlines(&cue.text)
            )
        })
        .collect();
    if blocks.is_empty() {
        String::new()
    } else {
        format!("{}\n", blocks.join("\n\n"))
    }
}

fn escape_vtt(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// WebVTT; a cue with word timings gets an inline timestamp before each
/// word after the first, which players use to style the words spoken so far.
pub fn write_vtt(cues: &[ExportCue]) -> String {
    let blocks: Vec<String> = cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            let body = if cue.words.is_empty() {
                escape_vtt(&normalise_newlines(&cue.text))
            } else {
                cue.words
                    .iter()
                    .enumerate()
                    .map(|(word_index, word)| {
                        if word_index == 0 {
                            escape_vtt(&word.text)
                        } else {
                            format!("<{}>{}", clock_time(word.start, '.'), escape_vtt(&word.text))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            format!("{}\n{} --> {}\n{}", index + 1, clock_time(cue.start, '.'), clock_time(cue.end, '.'), body)
        })
        .collect();
    if blocks.is_empty() {
        "WEBVTT\n\n".to_owned()
    } else {
        format!("WEBVTT\n\n{}\n", blocks.join("\n\n"))
    }
}

/// `H:MM:SS.cc` as ASS writes times.
fn ass_time(seconds: f64) -> String {
    let total = (seconds.max(0.0) * 100.0).round() as u64;
    let cs = total % 100;
    let s = (total / 100) % 60;
    let m = (total / 6000) % 60;
    let h = total / 360_000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// `&HAABBGGRR` from `#rrggbb` or `#rrggbbaa` (ASS alpha is 00 opaque).
pub fn ass_colour(css: &str) -> String {
    let hex = css.trim().trim_start_matches('#');
    let long: String = if hex.len() == 3 || hex.len() == 4 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_owned()
    };
    let valid = (long.len() == 6 || long.len() == 8) && long.chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return "&H00FFFFFF".to_owned();
    }
    let alpha = if long.len() == 8 { 255 - u8::from_str_radix(&long[6..8], 16).unwrap_or(0) } else { 0 };
    format!("&H{alpha:02X}{}{}{}", &long[4..6], &long[2..4], &long[0..2]).to_uppercase()
}

fn ass_escape(text: &str) -> String {
    normalise_newlines(text).replace('\n', "\\N").replace('{', "(").replace('}', ")")
}

/// ASS alignment (numpad layout) and vertical margin for a caption placed
/// `position_y` canvas pixels below the centre: the lower third is 1–3, the
/// upper third 7–9, the rest 4–6.
pub fn ass_placement(style: &ExportCueStyle, canvas_height: u32) -> (u32, u32) {
    let column = match style.text_align.as_str() {
        "left" => 1,
        "right" => 3,
        _ => 2,
    };
    let height = canvas_height as f64;
    let third = height / 6.0;
    let line_half = style.font_size * (height / FONT_SIZE_SCALE_REFERENCE) * 1.2 / 2.0;
    if style.position_y > third {
        (column, (height / 2.0 - style.position_y - line_half).round().max(0.0) as u32)
    } else if style.position_y < -third {
        (6 + column, (height / 2.0 + style.position_y - line_half).round().max(0.0) as u32)
    } else {
        (3 + column, 0)
    }
}

/// ASS with one style per distinct look and `\k` karaoke tags on cues that
/// carry words (each tag holds the word's duration in centiseconds; a pause
/// before a word becomes an empty syllable).
pub fn write_ass(cues: &[ExportCue], canvas: CanvasSize, title: &str) -> String {
    let mut styles: Vec<(String, String)> = Vec::new();
    let mut style_for = |style: &ExportCueStyle| -> String {
        let (alignment, margin_v) = ass_placement(style, canvas.height);
        let font_size = (style.font_size * (canvas.height as f64 / FONT_SIZE_SCALE_REFERENCE)).round() as i64;
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            style.font_family, font_size, style.color, style.bold, style.italic, alignment, margin_v
        );
        if let Some((name, _)) = styles.iter().find(|(_, line_key)| *line_key == key) {
            return name.clone();
        }
        let name = if styles.is_empty() { "Default".to_owned() } else { format!("Style{}", styles.len() + 1) };
        styles.push((name.clone(), key));
        let _ = (font_size, alignment, margin_v);
        name
    };
    let mut style_lines: Vec<String> = Vec::new();
    let mut events = Vec::with_capacity(cues.len());
    for cue in cues {
        let name = style_for(&cue.style);
        if !style_lines.iter().any(|line: &String| line.starts_with(&format!("Style: {name},"))) {
            let (alignment, margin_v) = ass_placement(&cue.style, canvas.height);
            let font_size = (cue.style.font_size * (canvas.height as f64 / FONT_SIZE_SCALE_REFERENCE)).round() as i64;
            style_lines.push(format!(
                "Style: {name},{},{font_size},{},&H00FFFF00,&H00000000,&H80000000,{},{},0,0,100,100,0,0,1,2,1,{alignment},20,20,{margin_v},1",
                cue.style.font_family.replace(',', " "),
                ass_colour(&cue.style.color),
                if cue.style.bold { "-1" } else { "0" },
                if cue.style.italic { "-1" } else { "0" },
            ));
        }
        let body = if cue.words.is_empty() {
            ass_escape(&cue.text)
        } else {
            let mut parts = String::new();
            let mut cursor = cue.start;
            let count = cue.words.len();
            for (index, word) in cue.words.iter().enumerate() {
                let gap = ((word.start - cursor) * 100.0).round() as i64;
                if gap > 0 {
                    parts.push_str(&format!("{{\\k{gap}}}"));
                }
                let from = word.start.max(cursor);
                let duration = ((word.end.max(word.start) - from) * 100.0).round().max(1.0) as i64;
                parts.push_str(&format!("{{\\k{duration}}}{}", ass_escape(&word.text)));
                if index + 1 < count {
                    parts.push(' ');
                }
                cursor = cursor.max(word.end);
            }
            parts
        };
        events.push(format!("Dialogue: 0,{},{},{name},,0,0,0,,{body}", ass_time(cue.start), ass_time(cue.end)));
    }
    if style_lines.is_empty() {
        style_lines.push(
            "Style: Default,Arial,48,&H00FFFFFF,&H00FFFF00,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,20,20,40,1"
                .to_owned(),
        );
    }
    let mut out = String::new();
    out.push_str("[Script Info]\n");
    out.push_str(&format!("Title: {}\n", title.replace(['\r', '\n'], " ")));
    out.push_str("ScriptType: v4.00+\n");
    out.push_str(&format!("PlayResX: {}\nPlayResY: {}\n", canvas.width, canvas.height));
    out.push_str("WrapStyle: 0\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\n");
    out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for line in &style_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for event in &events {
        out.push_str(event);
        out.push('\n');
    }
    out
}

/// A file name for the project's captions, keeping letters, digits, spaces
/// and dashes.
pub fn subtitle_file_name(name: &str, format: SubtitleFormat) -> String {
    let mut cleaned = String::new();
    let mut last_replaced = false;
    for c in name.chars() {
        if c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' {
            cleaned.push(c);
            last_replaced = false;
        } else if !last_replaced {
            cleaned.push('_');
            last_replaced = true;
        }
    }
    let trimmed = cleaned.trim();
    let stem = if trimmed.is_empty() { "captions" } else { trimmed };
    format!("{stem}.{}", format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CaptionWord, Element, Lane, Track};
    use time::MediaTime;

    fn style() -> ExportCueStyle {
        ExportCueStyle {
            font_family: "Inter".to_owned(),
            font_size: 15.0,
            color: "#ffffff".to_owned(),
            bold: true,
            italic: false,
            text_align: "center".to_owned(),
            position_y: 400.0,
        }
    }

    fn word(text: &str, start: f64, end: f64) -> ExportWord {
        ExportWord { text: text.to_owned(), start, end }
    }

    fn cues() -> Vec<ExportCue> {
        vec![
            ExportCue {
                text: "The quick brown fox".to_owned(),
                start: 0.5,
                end: 2.25,
                words: vec![word("The", 0.5, 0.8), word("quick", 0.9, 1.2), word("brown", 1.2, 1.6), word("fox", 1.6, 2.0)],
                style: style(),
            },
            ExportCue {
                text: "jumps over\nthe lazy dog & <cat>".to_owned(),
                start: 3661.004,
                end: 3663.0,
                words: Vec::new(),
                style: ExportCueStyle {
                    position_y: -400.0,
                    text_align: "left".to_owned(),
                    color: "#ff000080".to_owned(),
                    ..style()
                },
            },
        ]
    }

    #[test]
    fn srt_matches_the_web_writer() {
        assert_eq!(
            write_srt(&cues()),
            "1\n00:00:00,500 --> 00:00:02,250\nThe quick brown fox\n\n2\n01:01:01,004 --> 01:01:03,000\njumps over\nthe lazy dog & <cat>\n"
        );
        assert_eq!(write_srt(&[]), "");
    }

    #[test]
    fn vtt_matches_the_web_writer() {
        assert_eq!(
            write_vtt(&cues()),
            "WEBVTT\n\n1\n00:00:00.500 --> 00:00:02.250\nThe <00:00:00.900>quick <00:00:01.200>brown <00:00:01.600>fox\n\n2\n01:01:01.004 --> 01:01:03.000\njumps over\nthe lazy dog &amp; &lt;cat&gt;\n"
        );
    }

    #[test]
    fn ass_matches_the_web_writer() {
        let text = write_ass(&cues(), CanvasSize { width: 1080, height: 1920 }, "Fox");
        assert!(text.contains("Title: Fox\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920"));
        assert!(text.contains("Style: Default,Inter,320,&H00FFFFFF,&H00FFFF00,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,20,20,368,1"), "{text}");
        assert!(text.contains("Style: Style2,Inter,320,&H7F0000FF,"), "{text}");
        assert!(text.contains(",7,20,20,368,1\n"), "{text}");
        assert!(text.contains("Dialogue: 0,0:00:00.50,0:00:02.25,Default,,0,0,0,,{\\k30}The {\\k10}{\\k30}quick {\\k40}brown {\\k40}fox"), "{text}");
        assert!(text.contains("Dialogue: 0,1:01:01.00,1:01:03.00,Style2,,0,0,0,,jumps over\\Nthe lazy dog & <cat>"), "{text}");
        let empty = write_ass(&[], CanvasSize { width: 1920, height: 1080 }, "Empty");
        assert!(empty.contains("Style: Default,Arial,48,"));
    }

    #[test]
    fn colours_and_placement() {
        assert_eq!(ass_colour("#112233"), "&H00332211");
        assert_eq!(ass_colour("#fff"), "&H00FFFFFF");
        assert_eq!(ass_colour("#00000000"), "&HFF000000");
        assert_eq!(ass_colour("red"), "&H00FFFFFF");
        assert_eq!(ass_placement(&style(), 1080).0, 2);
        assert_eq!(ass_placement(&ExportCueStyle { position_y: 0.0, ..style() }, 1080), (5, 0));
        assert_eq!(ass_placement(&ExportCueStyle { position_y: -300.0, text_align: "right".to_owned(), ..style() }, 1080).0, 9);
        assert_eq!(subtitle_file_name("My promo: v2/final", SubtitleFormat::Vtt), "My promo_ v2_final.vtt");
        assert_eq!(subtitle_file_name("  ", SubtitleFormat::Srt), "captions.srt");
    }

    #[test]
    fn collects_visible_text_elements_with_words_on_the_timeline_clock() {
        let mut project = Project::new("Captions");
        let mut track = Track::new(TrackKind::Text, Lane::Overlay, "Captions");
        let mut caption = Element::text("hello world", MediaTime::from_seconds_f64(2.0).unwrap());
        caption.duration = MediaTime::from_seconds_f64(1.5).unwrap();
        caption.words = vec![
            CaptionWord { text: "hello".into(), start: MediaTime::ZERO, end: MediaTime::from_seconds_f64(0.5).unwrap() },
            CaptionWord {
                text: "world".into(),
                start: MediaTime::from_seconds_f64(0.6).unwrap(),
                end: MediaTime::from_seconds_f64(1.0).unwrap(),
            },
        ];
        caption.params.set("fontWeight", "bold").set("transform.positionY", 300.0);
        let mut hidden = Element::text("hidden", MediaTime::ZERO);
        hidden.hidden = true;
        let mut blank = Element::text("   ", MediaTime::from_seconds_f64(5.0).unwrap());
        blank.duration = MediaTime::ONE_TICK;
        track.elements = vec![caption, hidden, blank];
        project.tracks.push(track);

        let cues = collect_caption_cues(&project, None);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "hello world");
        assert_eq!((cues[0].start, cues[0].end), (2.0, 3.5));
        assert_eq!(cues[0].words[1], word("world", 2.6, 3.0));
        assert!(cues[0].style.bold);
        assert_eq!(cues[0].style.position_y, 300.0);
        assert!(collect_caption_cues(&project, Some("other")).is_empty());
        assert!(write_subtitles(&project, SubtitleFormat::Vtt, None).contains("hello <00:00:02.600>world"));
    }
}
