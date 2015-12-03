#version 330 core

// Vertex shader for rendering GPU draw commands in the framebuffer

in vec2 position;
in uvec2 fb_coord;

out vec2 frag_fb_coord;

void main() {
  gl_Position.xyzw = vec4(position, 0.0, 1.0);

  // Convert the PlayStation framebuffer coordinate into an OpenGL
  // texture coordinate
  float fb_x_coord = float(fb_coord.x) / 1024;
  float fb_y_coord = 1.0 - (float(fb_coord.y) / 512);

  frag_fb_coord = vec2(fb_x_coord, fb_y_coord);
}
