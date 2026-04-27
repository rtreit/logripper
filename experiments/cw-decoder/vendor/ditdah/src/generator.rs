// src/generator.rs
// Morse code WAV file generator for testing

use anyhow::Result;
use hound::{SampleFormat, WavSpec, WavWriter};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::path::Path;

pub struct MorseGenerator {
    sample_rate: u32,
    frequency: f32,
    _wpm: f32, // Stored for reference but not directly used in generation
    dot_duration: f32,
    dash_duration: f32,
    element_gap: f32,
    letter_gap: f32,
    word_gap: f32,
}

impl MorseGenerator {
    pub fn new(sample_rate: u32, frequency: f32, wpm: f32) -> Self {
        let dot_duration = 1.2 / wpm; // seconds per dot
        let dash_duration = 3.0 * dot_duration;
        let element_gap = dot_duration; // gap between dots/dashes
        let letter_gap = 3.0 * dot_duration; // gap between letters
        let word_gap = 7.0 * dot_duration; // gap between words

        Self {
            sample_rate,
            frequency,
            _wpm: wpm,
            dot_duration,
            dash_duration,
            element_gap,
            letter_gap,
            word_gap,
        }
    }

    pub fn generate_wav_file<P: AsRef<Path>>(&self, text: &str, path: P) -> Result<()> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(path, spec)?;
        let morse_code = self.text_to_morse(text);

        for &code in morse_code.iter() {
            match code {
                MorseElement::Dot => self.write_tone(&mut writer, self.dot_duration)?,
                MorseElement::Dash => self.write_tone(&mut writer, self.dash_duration)?,
                MorseElement::ElementGap => self.write_silence(&mut writer, self.element_gap)?,
                MorseElement::LetterGap => self.write_silence(&mut writer, self.letter_gap)?,
                MorseElement::WordGap => self.write_silence(&mut writer, self.word_gap)?,
            }
        }

        writer.finalize()?;
        Ok(())
    }

    fn write_tone<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut WavWriter<W>,
        duration: f32,
    ) -> Result<()> {
        let samples = (duration * self.sample_rate as f32) as usize;
        for i in 0..samples {
            let t = i as f32 / self.sample_rate as f32;
            let sample = (2.0 * PI * self.frequency * t).sin();
            let amplitude = 0.5; // 50% amplitude to avoid clipping
            writer.write_sample((sample * amplitude * i16::MAX as f32) as i16)?;
        }
        Ok(())
    }

    fn write_silence<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut WavWriter<W>,
        duration: f32,
    ) -> Result<()> {
        let samples = (duration * self.sample_rate as f32) as usize;
        for _ in 0..samples {
            writer.write_sample(0i16)?;
        }
        Ok(())
    }

    fn text_to_morse(&self, text: &str) -> Vec<MorseElement> {
        let morse_map = get_morse_map();
        let mut result = Vec::new();
        let words: Vec<&str> = text.split_whitespace().collect();

        for (word_idx, word) in words.iter().enumerate() {
            for (char_idx, ch) in word.chars().enumerate() {
                if let Some(morse_str) = morse_map.get(&ch.to_ascii_uppercase()) {
                    for (elem_idx, morse_char) in morse_str.chars().enumerate() {
                        match morse_char {
                            '.' => result.push(MorseElement::Dot),
                            '-' => result.push(MorseElement::Dash),
                            _ => {}
                        }
                        // Add element gap between dots/dashes (except after last element)
                        if elem_idx < morse_str.len() - 1 {
                            result.push(MorseElement::ElementGap);
                        }
                    }
                }
                // Add letter gap between letters (except after last letter)
                if char_idx < word.len() - 1 {
                    result.push(MorseElement::LetterGap);
                }
            }
            // Add word gap between words (except after last word)
            if word_idx < words.len() - 1 {
                result.push(MorseElement::WordGap);
            }
        }

        result
    }
}

#[derive(Debug, Clone, Copy)]
enum MorseElement {
    Dot,
    Dash,
    ElementGap,
    LetterGap,
    WordGap,
}

fn get_morse_map() -> HashMap<char, &'static str> {
    [
        ('A', ".-"),
        ('B', "-..."),
        ('C', "-.-."),
        ('D', "-.."),
        ('E', "."),
        ('F', "..-."),
        ('G', "--."),
        ('H', "...."),
        ('I', ".."),
        ('J', ".---"),
        ('K', "-.-"),
        ('L', ".-.."),
        ('M', "--"),
        ('N', "-."),
        ('O', "---"),
        ('P', ".--."),
        ('Q', "--.-"),
        ('R', ".-."),
        ('S', "..."),
        ('T', "-"),
        ('U', "..-"),
        ('V', "...-"),
        ('W', ".--"),
        ('X', "-..-"),
        ('Y', "-.--"),
        ('Z', "--.."),
        ('1', ".----"),
        ('2', "..---"),
        ('3', "...--"),
        ('4', "....-"),
        ('5', "....."),
        ('6', "-...."),
        ('7', "--..."),
        ('8', "---.."),
        ('9', "----."),
        ('0', "-----"),
    ]
    .iter()
    .cloned()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morse_generation() {
        let generator = MorseGenerator::new(12000, 600.0, 20.0);
        let result = generator.generate_wav_file("SOS", "test_sos.wav");
        assert!(result.is_ok());

        // Clean up test file
        std::fs::remove_file("test_sos.wav").ok();
    }
}
