//! One length type. Callers pass [`Pt`], [`Mm`], [`Cm`], [`Inch`], [`Twips`], or [`Emu`].
//! The model stores twips for text metrics and EMU for drawings.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Twips(pub i32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emu(pub i64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt(pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mm(pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cm(pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inch(pub f32);

pub trait IntoTwips {
    fn twips(self) -> Twips;
}

impl IntoTwips for Twips {
    fn twips(self) -> Twips {
        self
    }
}

impl IntoTwips for Pt {
    fn twips(self) -> Twips {
        Twips((self.0 * 20.0).round() as i32)
    }
}

impl IntoTwips for Mm {
    fn twips(self) -> Twips {
        Twips((self.0 / 25.4 * 1440.0).round() as i32)
    }
}

impl IntoTwips for Cm {
    fn twips(self) -> Twips {
        Mm(self.0 * 10.0).twips()
    }
}

impl IntoTwips for Inch {
    fn twips(self) -> Twips {
        Twips((self.0 * 1440.0).round() as i32)
    }
}

pub trait IntoEmu {
    fn emu(self) -> Emu;
}

impl IntoEmu for Emu {
    fn emu(self) -> Emu {
        self
    }
}

impl IntoEmu for Twips {
    fn emu(self) -> Emu {
        Emu(i64::from(self.0) * 635)
    }
}

impl IntoEmu for Pt {
    fn emu(self) -> Emu {
        Emu((f64::from(self.0) * 12_700.0).round() as i64)
    }
}

impl IntoEmu for Mm {
    fn emu(self) -> Emu {
        Emu((f64::from(self.0) * 36_000.0).round() as i64)
    }
}

impl IntoEmu for Cm {
    fn emu(self) -> Emu {
        Emu((f64::from(self.0) * 360_000.0).round() as i64)
    }
}

impl IntoEmu for Inch {
    fn emu(self) -> Emu {
        Emu((f64::from(self.0) * 914_400.0).round() as i64)
    }
}

impl Twips {
    pub fn to_pt(self) -> f32 {
        self.0 as f32 / 20.0
    }
}

impl Emu {
    pub fn to_twips(self) -> Twips {
        Twips((self.0 / 635) as i32)
    }

    pub fn to_pt(self) -> f32 {
        (self.0 as f64 / 12_700.0) as f32
    }
}

pub fn a4() -> (Twips, Twips) {
    (Mm(210.0).twips(), Mm(297.0).twips())
}

pub fn letter() -> (Twips, Twips) {
    (Inch(8.5).twips(), Inch(11.0).twips())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetre_to_twips() -> Result<(), String> {
        let tw = Mm(20.0).twips();
        if tw.0 < 1100 || tw.0 > 1200 {
            return Err(format!("20 mm is {} twips", tw.0));
        }
        Ok(())
    }

    #[test]
    fn one_inch_is_ninety_six_pixels_of_emu() -> Result<(), String> {
        if Inch(1.0).emu().0 != 914_400 {
            return Err(format!("one inch is {} EMU", Inch(1.0).emu().0));
        }
        if Emu(914_400).to_twips().0 != 1440 {
            return Err("one inch is not 1440 twips".into());
        }
        Ok(())
    }
}
