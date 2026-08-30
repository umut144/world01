use bevy::{
    asset::{load_internal_asset, uuid_handle},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef},
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

const PROJECTION_DEPTH_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("41caa612-7608-4bb0-80c0-aa418ba2c56a");

pub struct ProjectionDepthPresentationPlugin;

impl Plugin for ProjectionDepthPresentationPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            PROJECTION_DEPTH_SHADER_HANDLE,
            "../../../assets/shaders/projection_depth.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(Material2dPlugin::<ProjectionDepthMaterial>::default());
    }
}

#[derive(Debug, Clone, Copy, ShaderType)]
struct ProjectionDepthUniform {
    color: Vec4,
    deformation_offset: Vec2,
    deformation_pivot: Vec2,
    deformation_extent: f32,
    authored_layer: f32,
    presentation_layer: f32,
    projection_depth_meters: f32,
    geometric_depth_scale: f32,
    geometric_depth_bias: f32,
    padding: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ProjectionDepthMaterial {
    #[uniform(0)]
    uniform: ProjectionDepthUniform,
}

impl ProjectionDepthMaterial {
    pub fn from_color(
        color: Color,
        authored_layer: f32,
        presentation_layer: f32,
        projection_depth_meters: f32,
    ) -> Self {
        Self {
            uniform: ProjectionDepthUniform {
                color: color.to_linear().to_vec4(),
                deformation_offset: Vec2::ZERO,
                deformation_pivot: Vec2::ZERO,
                deformation_extent: 1.0,
                authored_layer,
                presentation_layer,
                projection_depth_meters,
                geometric_depth_scale: 0.0,
                geometric_depth_bias: 0.0,
                padding: 0.0,
            },
        }
    }

    pub fn set_geometric_depth(&mut self, scale: f32, bias: f32) {
        self.uniform.geometric_depth_scale = scale.max(0.0);
        self.uniform.geometric_depth_bias = bias;
    }

    pub fn set_deformation(&mut self, offset: Vec2, pivot: Vec2, extent: f32) {
        self.uniform.deformation_offset = offset;
        self.uniform.deformation_pivot = pivot;
        self.uniform.deformation_extent = extent.max(f32::EPSILON);
    }

    pub fn set_presentation_layer(&mut self, layer: f32) {
        self.uniform.presentation_layer = layer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometric_depth_is_opt_in_and_keeps_a_separate_surface_bias() {
        let mut material = ProjectionDepthMaterial::from_color(Color::WHITE, 0.02, -1.0, 0.4);

        assert_eq!(material.uniform.geometric_depth_scale, 0.0);
        assert_eq!(material.uniform.geometric_depth_bias, 0.0);

        material.set_geometric_depth(0.004, 0.0001);

        assert_eq!(material.uniform.geometric_depth_scale, 0.004);
        assert_eq!(material.uniform.geometric_depth_bias, 0.0001);
    }
}

impl Material2d for ProjectionDepthMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(PROJECTION_DEPTH_SHADER_HANDLE.clone())
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PROJECTION_DEPTH_SHADER_HANDLE.clone())
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}
