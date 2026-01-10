use anyhow::Result;

pub fn parse_notes(input: &str) -> Result<Vec<u8>> {
    let input = input.trim();

    // Check if it's a range like "C2..C6"
    if input.contains("..") {
        let parts: Vec<&str> = input.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid note range format: {}", input);
        }
        let start = note_name_to_midi(parts[0])?;
        let end = note_name_to_midi(parts[1])?;
        return Ok((start..=end).collect());
    }

    // Check if it's a comma-separated list
    if input.contains(',') {
        let mut notes = Vec::new();
        for part in input.split(',') {
            let note = note_name_to_midi(part.trim())?;
            notes.push(note);
        }
        return Ok(notes);
    }

    // Single note
    let note = note_name_to_midi(input)?;
    Ok(vec![note])
}

pub fn parse_velocities(input: &str) -> Result<Vec<u8>> {
    let input = input.trim();

    // Check for range with step like "127..1:8"
    if input.contains("..") && input.contains(':') {
        let parts: Vec<&str> = input.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid velocity range format: {}", input);
        }
        let start: u8 = parts[0].parse()?;
        let rest: Vec<&str> = parts[1].split(':').collect();
        if rest.len() != 2 {
            anyhow::bail!("Invalid velocity range format: {}", input);
        }
        let end: u8 = rest[0].parse()?;
        let step: u8 = rest[1].parse()?;

        let mut velocities = Vec::new();
        if start > end {
            // Descending
            let mut v = start;
            loop {
                velocities.push(v);
                if v <= end || v < step {
                    break;
                }
                v = v.saturating_sub(step);
                if v < end {
                    break;
                }
            }
        } else {
            // Ascending
            let mut v = start;
            while v <= end {
                velocities.push(v);
                if v + step > 127 {
                    break;
                }
                v += step;
            }
        }
        return Ok(velocities);
    }

    // Check for simple range like "64..127"
    if input.contains("..") {
        let parts: Vec<&str> = input.split("..").collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid velocity range format: {}", input);
        }
        let start: u8 = parts[0].parse()?;
        let end: u8 = parts[1].parse()?;
        return Ok((start..=end).collect());
    }

    // Check if it's a comma-separated list
    if input.contains(',') {
        let mut velocities = Vec::new();
        for part in input.split(',') {
            let vel: u8 = part.trim().parse()?;
            velocities.push(vel);
        }
        return Ok(velocities);
    }

    // Single velocity
    let vel: u8 = input.parse()?;
    Ok(vec![vel])
}

fn note_name_to_midi(name: &str) -> Result<u8> {
    let name = name.trim().to_uppercase();

    // Parse note name and octave
    let note_part = if name.len() >= 2
        && (name.chars().nth(1) == Some('#') || name.chars().nth(1) == Some('S'))
    {
        &name[..2]
    } else {
        &name[..1]
    };

    let octave_part = &name[note_part.len()..];
    let octave: i32 = octave_part
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid octave in note: {}", name))?;

    let pitch = match note_part {
        "C" => 0,
        "C#" | "CS" | "DB" => 1,
        "D" => 2,
        "D#" | "DS" | "EB" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "FS" | "GB" => 6,
        "G" => 7,
        "G#" | "GS" | "AB" => 8,
        "A" => 9,
        "A#" | "AS" | "BB" => 10,
        "B" => 11,
        _ => anyhow::bail!("Invalid note name: {}", note_part),
    };

    let midi = (octave + 1) * 12 + pitch;

    if midi < 0 || midi > 127 {
        anyhow::bail!("MIDI note out of range: {}", midi);
    }

    Ok(midi as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_note() {
        assert_eq!(parse_notes("C4").unwrap(), vec![60]);
        assert_eq!(parse_notes("A4").unwrap(), vec![69]);
    }

    #[test]
    fn test_parse_note_range() {
        let notes = parse_notes("C4..E4").unwrap();
        assert_eq!(notes, vec![60, 61, 62, 63, 64]);
    }

    #[test]
    fn test_parse_note_list() {
        let notes = parse_notes("C4,E4,G4").unwrap();
        assert_eq!(notes, vec![60, 64, 67]);
    }

    #[test]
    fn test_parse_velocities_range_step() {
        let vels = parse_velocities("127..1:8").unwrap();
        assert!(vels.contains(&127));
        assert!(vels.contains(&119));
        assert_eq!(vels[0], 127);
    }

    #[test]
    fn test_parse_velocities_list() {
        let vels = parse_velocities("127,100,64").unwrap();
        assert_eq!(vels, vec![127, 100, 64]);
    }
}