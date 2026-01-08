use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, 
Deserialize)]
pub struct MidiNote(pub u8);

impl MidiNote {
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_uppercase();
        let note_name = name.chars().next()?;
        let rest = &name[1..];
        
        let mut accidental = 0i8;
        let mut octave_str = rest;
        
        if rest.starts_with('#') || rest.starts_with('S') {
            accidental = 1;
            octave_str = &rest[1..];
        } else if rest.starts_with('B') && rest.len() > 1 {
            accidental = -1;
            octave_str = &rest[1..];
        }
        
        let octave: i8 = octave_str.parse().ok()?;
        
        let note_offset = match note_name {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        
        let midi_num = (octave + 1) * 12 + note_offset + accidental;
        
        if (0..=127).contains(&midi_num) {
            Some(MidiNote(midi_num as u8))
        } else {
            None
        }
    }
    
    pub fn to_name(self) -> String {
        let octave = (self.0 as i32 / 12) - 1;
        let note = self.0 % 12;
        let name = match note {
            0 => "C",
            1 => "C#",
            2 => "D",
            3 => "D#",
            4 => "E",
            5 => "F",
            6 => "F#",
            7 => "G",
            8 => "G#",
            9 => "A",
            10 => "A#",
            11 => "B",
            _ => unreachable!(),
        };
        format!("{}{}", name, octave)
    }
}

impl fmt::Display for MidiNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_name())
    }
}

pub fn parse_notes(spec: &str) -> anyhow::Result<Vec<MidiNote>> {
    if spec.contains("..") {
        // Range: "C2..C6"
        let parts: Vec<&str> = spec.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid note range format: {}", spec);
        }
        
        let start = MidiNote::from_name(parts[0])
            .ok_or_else(|| anyhow::anyhow!("Invalid start note: {}", 
parts[0]))?;
        let end = MidiNote::from_name(parts[1])
            .ok_or_else(|| anyhow::anyhow!("Invalid end note: {}", 
parts[1]))?;
        
        if start.0 > end.0 {
            anyhow::bail!("Start note must be <= end note");
        }
        
        Ok((start.0..=end.0).map(MidiNote).collect())
    } else if spec.contains(',') {
        // List: "C2,D2,E2"
        spec.split(',')
            .map(|s| {
                MidiNote::from_name(s.trim())
                    .ok_or_else(|| anyhow::anyhow!("Invalid note: {}", s))
            })
            .collect()
    } else {
        // Single note
        let note = MidiNote::from_name(spec)
            .ok_or_else(|| anyhow::anyhow!("Invalid note: {}", spec))?;
        Ok(vec![note])
    }
}

pub fn parse_velocities(spec: &str) -> anyhow::Result<Vec<u8>> {
    if spec.contains("..") {
        // Range with optional step: "127..1:8" or "1..127"
        let (range_part, step) = if spec.contains(':') {
            let parts: Vec<&str> = spec.split(':').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid velocity range format: {}", spec);
            }
            (parts[0], parts[1].parse::<u8>()?)
        } else {
            (spec, 1u8)
        };
        
        let parts: Vec<&str> = range_part.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid velocity range format: {}", spec);
        }
        
        let start: u8 = parts[0].parse()?;
        let end: u8 = parts[1].parse()?;
        
        if start == 0 || end == 0 || start > 127 || end > 127 {
            anyhow::bail!("Velocity must be 1-127");
        }
        
        let mut velocities = Vec::new();
        if start <= end {
            let mut v = start;
            while v <= end {
                velocities.push(v);
                if v > end - step {
                    break;
                }
                v += step;
            }
        } else {
            let mut v = start;
            loop {
                velocities.push(v);
                if v < end + step {
                    break;
                }
                v = v.saturating_sub(step);
            }
        }
        
        Ok(velocities)
    } else if spec.contains(',') {
        // List: "127,100,80"
        spec.split(',')
            .map(|s| {
                let v: u8 = s.trim().parse()?;
                if v == 0 || v > 127 {
                    anyhow::bail!("Velocity must be 1-127");
                }
                Ok(v)
            })
            .collect()
    } else {
        // Single velocity
        let v: u8 = spec.parse()?;
        if v == 0 || v > 127 {
            anyhow::bail!("Velocity must be 1-127");
        }
        Ok(vec![v])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleJob {
    pub note: MidiNote,
    pub velocity: u8,
    pub rr_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleMetadata {
    pub job: SampleJob,
    pub filename: String,
    pub peak_db: f32,
    pub duration_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub config: crate::cli::RunConfig,
    pub samples: Vec<SampleMetadata>,
}

pub fn format_filename(prefix: &str, note: MidiNote, velocity: u8, rr: 
u32, ext: &str) -> String {
    format!(
        "{}_{}_{:03}_vel{:03}_rr{:02}.{}",
        prefix,
        note.to_name().replace('#', "s"),
        note.0,
        velocity,
        rr,
        ext
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_parsing() {
        assert_eq!(MidiNote::from_name("C4").unwrap().0, 60);
        assert_eq!(MidiNote::from_name("A4").unwrap().0, 69);
        assert_eq!(MidiNote::from_name("C#4").unwrap().0, 61);
        assert_eq!(MidiNote::from_name("Bb4").unwrap().0, 70);
    }

    #[test]
    fn test_note_range() {
        let notes = parse_notes("C4..E4").unwrap();
        assert_eq!(notes.len(), 5); // C, C#, D, D#, E
        assert_eq!(notes[0].0, 60);
        assert_eq!(notes[4].0, 64);
    }

    #[test]
    fn test_velocity_range() {
        let vels = parse_velocities("127..100:10").unwrap();
        assert_eq!(vels, vec![127, 117, 107]);
    }

    #[test]
    fn test_velocity_list() {
        let vels = parse_velocities("127,100,80").unwrap();
        assert_eq!(vels, vec![127, 100, 80]);
    }

    #[test]
    fn test_filename_format() {
        let name = format_filename("Piano", MidiNote(60), 127, 1, "wav");
        assert_eq!(name, "Piano_C4_060_vel127_rr01.wav");
    }
}
