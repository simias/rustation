#version 330 core

// Vertex shader for rendering GPU draw commands in the framebuffer

in ivec2 position;
in vec3 color;
in uvec2 texture_page;
in uvec2 texture_coord;
in uvec2 clut;
in int texture_blend_mode;
in int depth_shift;
in int dither;

// Drawing offset
uniform ivec2 offset;

out vec3 frag_shading_color;
flat out uvec2 frag_texture_page;
out vec2 frag_texture_coord;
flat out uvec2 frag_clut;
flat out int frag_texture_blend_mode;
flat out int frag_depth_shift;
flat out int frag_dither;

void main() {
  ivec2 pos = position + offset;

  // Convert VRAM coordinates (0;1023, 0;511) into OpenGL coordinates
  // (-1;1, -1;1)
  float xpos = (float(pos.x) / 512) - 1.0;
  // VRAM puts 0 at the top, OpenGL at the bottom, we must mirror
  // vertically
  float ypos = 1.0 - (float(pos.y) / 256);

  gl_Position.xyzw = vec4(xpos, ypos, 0.0, 1.0);

  // Glium doesn't support "normalized" for now
  frag_shading_color = vec3(color / 255.);

  // Let OpenGL interpolate the texel position
  frag_texture_coord = vec2(texture_coord);

  frag_texture_page = texture_page;
  frag_clut = clut;
  frag_texture_blend_mode = texture_blend_mode;
  frag_depth_shift = depth_shift;
  frag_dither = dither;
}
