use bevy::math::Vec3;

#[derive(Clone, Debug)]
pub struct Gyroscope {
    pub spin_rate: f32,
    pub precession_rate: f32,
    pub asymmetry: f32,
    pub angular_momentum: Vec3,
}

impl Default for Gyroscope {
    fn default() -> Self {
        Self::new()
    }
}

impl Gyroscope {
    pub fn new() -> Self {
        Self {
            spin_rate: 0.0,
            precession_rate: 0.0,
            asymmetry: 0.0,
            angular_momentum: Vec3::ZERO,
        }
    }

    pub fn update_params(&mut self, rpm: f32, precession_hz: f32, asymmetry: f32) {
        self.spin_rate = rpm * (std::f32::consts::TAU / 60.0);
        self.precession_rate = precession_hz * std::f32::consts::TAU;
        self.asymmetry = asymmetry;
    }

    pub fn update_angular_momentum(&mut self, spin_axis: Vec3) {
        self.angular_momentum = self.spin_rate * spin_axis;
        // Asymmetry tilts the momentum vector slightly
        self.angular_momentum += self.asymmetry * Vec3::new(0.0, 0.1 * self.spin_rate, 0.0);
    }
}
