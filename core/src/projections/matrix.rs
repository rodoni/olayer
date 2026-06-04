#![allow(clippy::return_self_not_must_use)]

use std::ops::Mul;

/// A 4×4 matrix in column-major order, suitable for GLSL/WGSL uniform buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix4 {
    m: [f32; 16],
}

impl Default for Matrix4 {
    #[inline]
    fn default() -> Self {
        Self::identity()
    }
}

impl Matrix4 {
    /// Generates an identity matrix.
    #[inline]
    pub fn identity() -> Self {
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0, // Col 0
                0.0, 1.0, 0.0, 0.0, // Col 1
                0.0, 0.0, 1.0, 0.0, // Col 2
                0.0, 0.0, 0.0, 1.0, // Col 3
            ],
        }
    }

    /// Generates an orthographic projection matrix.
    #[inline]
    pub fn ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let r_l = right - left;
        let t_b = top - bottom;
        let f_n = far - near;

        let mut m = [0.0; 16];
        m[0] = 2.0 / r_l;
        m[5] = 2.0 / t_b;
        m[10] = -2.0 / f_n;
        m[12] = -(right + left) / r_l;
        m[13] = -(top + bottom) / t_b;
        m[14] = -(far + near) / f_n;
        m[15] = 1.0;

        Self { m }
    }

    /// Generates a translation matrix.
    #[inline]
    pub fn translation(tx: f32, ty: f32, tz: f32) -> Self {
        let mut m = Self::identity().m;
        m[12] = tx;
        m[13] = ty;
        m[14] = tz;
        Self { m }
    }

    /// Generates a rotation matrix around the Z axis.
    #[inline]
    pub fn rotation_z(angle_rad: f32) -> Self {
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();
        let mut m = Self::identity().m;
        m[0] = cos_a;
        m[1] = sin_a;
        m[4] = -sin_a;
        m[5] = cos_a;
        Self { m }
    }

    /// Returns a shared reference to the underlying column-major array.
    #[inline]
    pub const fn as_slice(&self) -> &[f32; 16] {
        &self.m
    }

    /// Returns a mutable reference to the underlying column-major array.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [f32; 16] {
        &mut self.m
    }

    /// Consumes the matrix and returns the underlying column-major array.
    #[inline]
    pub const fn into_array(self) -> [f32; 16] {
        self.m
    }

    /// Multiplies this matrix with another Matrix4 (Self * Other).
    #[inline]
    pub fn multiply(&self, other: &Self) -> Self {
        let a = &self.m;
        let b = &other.m;
        let mut m = [0.0; 16];

        // Column 0
        m[0]  = a[0]*b[0] + a[4]*b[1] + a[8]*b[2]  + a[12]*b[3];
        m[1]  = a[1]*b[0] + a[5]*b[1] + a[9]*b[2]  + a[13]*b[3];
        m[2]  = a[2]*b[0] + a[6]*b[1] + a[10]*b[2] + a[14]*b[3];
        m[3]  = a[3]*b[0] + a[7]*b[1] + a[11]*b[2] + a[15]*b[3];

        // Column 1
        m[4]  = a[0]*b[4] + a[4]*b[5] + a[8]*b[6]  + a[12]*b[7];
        m[5]  = a[1]*b[4] + a[5]*b[5] + a[9]*b[6]  + a[13]*b[7];
        m[6]  = a[2]*b[4] + a[6]*b[5] + a[10]*b[6] + a[14]*b[7];
        m[7]  = a[3]*b[4] + a[7]*b[5] + a[11]*b[6] + a[15]*b[7];

        // Column 2
        m[8]  = a[0]*b[8] + a[4]*b[9] + a[8]*b[10]  + a[12]*b[11];
        m[9]  = a[1]*b[8] + a[5]*b[9] + a[9]*b[10]  + a[13]*b[11];
        m[10] = a[2]*b[8] + a[6]*b[9] + a[10]*b[10] + a[14]*b[11];
        m[11] = a[3]*b[8] + a[7]*b[9] + a[11]*b[10] + a[15]*b[11];

        // Column 3
        m[12] = a[0]*b[12] + a[4]*b[13] + a[8]*b[14]  + a[12]*b[15];
        m[13] = a[1]*b[12] + a[5]*b[13] + a[9]*b[14]  + a[13]*b[15];
        m[14] = a[2]*b[12] + a[6]*b[13] + a[10]*b[14] + a[14]*b[15];
        m[15] = a[3]*b[12] + a[7]*b[13] + a[11]*b[14] + a[15]*b[15];

        Self { m }
    }
}

impl Mul<&Matrix4> for &Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn mul(self, rhs: &Matrix4) -> Self::Output {
        self.multiply(rhs)
    }
}

impl Mul<Matrix4> for Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn mul(self, rhs: Matrix4) -> Self::Output {
        self.multiply(&rhs)
    }
}

impl Mul<&Matrix4> for Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn mul(self, rhs: &Matrix4) -> Self::Output {
        self.multiply(rhs)
    }
}

impl Mul<Matrix4> for &Matrix4 {
    type Output = Matrix4;
    #[inline]
    fn mul(self, rhs: Matrix4) -> Self::Output {
        self.multiply(&rhs)
    }
}
