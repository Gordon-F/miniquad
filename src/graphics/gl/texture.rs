use crate::{
    sapp::*, Context, FilterMode, GraphicTexture, TextureAccess, TextureFormat, TextureParams,
    TextureWrap,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Texture {
    pub(crate) texture: GLuint,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

impl Texture {
    pub fn empty() -> Texture {
        Texture {
            texture: 0,
            width: 0,
            height: 0,
            format: TextureFormat::RGBA8,
        }
    }

    pub fn gl_internal_id(&self) -> GLuint {
        self.texture
    }

    /// Delete GPU texture, leaving handle unmodified.
    ///
    /// More high-level code on top of miniquad probably is going to call this in Drop implementation of some
    /// more RAII buffer object.
    ///
    /// There is no protection against using deleted textures later. However its not an UB in OpenGl and thats why
    /// this function is not marked as unsafe
    pub fn delete(&self) {
        unsafe {
            glDeleteTextures(1, &self.texture as *const _);
        }
    }
}

/// Converts from TextureFormat to (internal_format, format, pixel_type)
impl From<TextureFormat> for (GLenum, GLenum, GLenum) {
    fn from(format: TextureFormat) -> Self {
        match format {
            TextureFormat::RGB8 => (GL_RGB, GL_RGB, GL_UNSIGNED_BYTE),
            TextureFormat::RGBA8 => (GL_RGBA, GL_RGBA, GL_UNSIGNED_BYTE),
            TextureFormat::Depth => (GL_DEPTH_COMPONENT, GL_DEPTH_COMPONENT, GL_UNSIGNED_SHORT),
            #[cfg(target_arch = "wasm32")]
            TextureFormat::Alpha => (GL_ALPHA, GL_ALPHA, GL_UNSIGNED_BYTE),
            #[cfg(not(target_arch = "wasm32"))]
            TextureFormat::Alpha => (GL_R8, GL_RED, GL_UNSIGNED_BYTE), // texture updates will swizzle Red -> Alpha to match WASM
        }
    }
}

impl GraphicTexture for Texture {
    fn new_render_texture(ctx: &mut Context, params: TextureParams) -> Texture {
        Self::new(ctx, TextureAccess::RenderTarget, None, params)
    }

    fn new(
        ctx: &mut Context,
        _access: TextureAccess,
        bytes: Option<&[u8]>,
        params: TextureParams,
    ) -> Texture {
        if let Some(bytes_data) = bytes {
            assert_eq!(
                params.format.size(params.width, params.height) as usize,
                bytes_data.len()
            );
        }

        let (internal_format, format, pixel_type) = params.format.into();

        ctx.cache.store_texture_binding(0);

        let mut texture: GLuint = 0;

        unsafe {
            glGenTextures(1, &mut texture as *mut _);
            ctx.cache.bind_texture(0, texture);
            glPixelStorei(GL_UNPACK_ALIGNMENT, 1); // miniquad always uses row alignment of 1

            if cfg!(not(target_arch = "wasm32")) {
                // if not WASM
                if params.format == TextureFormat::Alpha {
                    // if alpha miniquad texture, the value on non-WASM is stored in red channel
                    // swizzle red -> alpha
                    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_A, GL_RED as _);
                } else {
                    // keep alpha -> alpha
                    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_A, GL_ALPHA as _);
                }
            }

            glTexImage2D(
                GL_TEXTURE_2D,
                0,
                internal_format as i32,
                params.width as i32,
                params.height as i32,
                0,
                format,
                pixel_type,
                match bytes {
                    Some(bytes) => bytes.as_ptr() as *const _,
                    Option::None => std::ptr::null(),
                },
            );

            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
            glTexParameteri(
                GL_TEXTURE_2D,
                GL_TEXTURE_MIN_FILTER,
                params.filter.type_() as i32,
            );
            glTexParameteri(
                GL_TEXTURE_2D,
                GL_TEXTURE_MAG_FILTER,
                params.filter.type_() as i32,
            );
        }
        ctx.cache.restore_texture_binding(0);

        Texture {
            texture,
            width: params.width,
            height: params.height,
            format: params.format,
        }
    }

    fn from_data_and_format(ctx: &mut Context, bytes: &[u8], params: TextureParams) -> Texture {
        Self::new(ctx, TextureAccess::Static, Some(bytes), params)
    }

    fn from_rgba8(ctx: &mut Context, width: u16, height: u16, bytes: &[u8]) -> Texture {
        assert_eq!(width as usize * height as usize * 4, bytes.len());

        Self::from_data_and_format(
            ctx,
            bytes,
            TextureParams {
                width: width as _,
                height: height as _,
                format: TextureFormat::RGBA8,
                wrap: TextureWrap::Clamp,
                filter: FilterMode::Linear,
            },
        )
    }

    fn set_filter(&self, ctx: &mut Context, filter: FilterMode) {
        ctx.cache.store_texture_binding(0);
        ctx.cache.bind_texture(0, self.texture);
        unsafe {
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, filter as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, filter as i32);
        }
        ctx.cache.restore_texture_binding(0);
    }

    fn resize(&mut self, ctx: &mut Context, width: u32, height: u32, bytes: Option<&[u8]>) {
        ctx.cache.store_texture_binding(0);

        let (internal_format, format, pixel_type) = self.format.into();

        self.width = width;
        self.height = height;

        unsafe {
            glTexImage2D(
                GL_TEXTURE_2D,
                0,
                internal_format as i32,
                self.width as i32,
                self.height as i32,
                0,
                format,
                pixel_type,
                match bytes {
                    Some(bytes) => bytes.as_ptr() as *const _,
                    Option::None => std::ptr::null(),
                },
            );
        }

        ctx.cache.restore_texture_binding(0);
    }

    fn update(&self, ctx: &mut Context, bytes: &[u8]) {
        assert_eq!(self.size(self.width, self.height), bytes.len());

        self.update_texture_part(
            ctx,
            0 as _,
            0 as _,
            self.width as _,
            self.height as _,
            bytes,
        )
    }

    fn update_texture_part(
        &self,
        ctx: &mut Context,
        x_offset: i32,
        y_offset: i32,
        width: i32,
        height: i32,
        bytes: &[u8],
    ) {
        assert_eq!(self.size(width as _, height as _), bytes.len());
        assert!(x_offset + width <= self.width as _);
        assert!(y_offset + height <= self.height as _);

        ctx.cache.store_texture_binding(0);
        ctx.cache.bind_texture(0, self.texture);

        let (_, format, pixel_type) = self.format.into();

        unsafe {
            glPixelStorei(GL_UNPACK_ALIGNMENT, 1); // miniquad always uses row alignment of 1

            if cfg!(not(target_arch = "wasm32")) {
                // if not WASM
                if self.format == TextureFormat::Alpha {
                    // if alpha miniquad texture, the value on non-WASM is stored in red channel
                    // swizzle red -> alpha
                    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_A, GL_RED as _);
                } else {
                    // keep alpha -> alpha
                    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_A, GL_ALPHA as _);
                }
            }

            glTexSubImage2D(
                GL_TEXTURE_2D,
                0,
                x_offset as _,
                y_offset as _,
                width as _,
                height as _,
                format,
                pixel_type,
                bytes.as_ptr() as *const _,
            );
        }

        ctx.cache.restore_texture_binding(0);
    }

    #[inline]
    fn size(&self, width: u32, height: u32) -> usize {
        self.format.size(width, height) as usize
    }
}
