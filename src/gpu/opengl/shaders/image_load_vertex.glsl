#version 330 core

// Vertex shader for uploading textures into the framebuffer

in uvec2 position;
in vec2 image_coord;

out vec2 frag_image_coord;

void main() {
  // Convert VRAM position into OpenGL coordinates
  float xpos = (float(position.x) / 512) - 1.0;
  float ypos = 1.0 - (float(position.y) / 256);

  gl_Position.xyzw = vec4(xpos, ypos, 0.0, 1.0);

  frag_image_coord = vec2(image_coord);
}
