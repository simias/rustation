#version 330 core

in ivec2 vertex_position;
in vec3 vertex_color;

// Drawing offset
uniform ivec2 offset;

out vec3 color;

void main() {
  ivec2 position = vertex_position + offset;

  // Convert VRAM coordinates (0;1023, 0;511) into OpenGL coordinates
  // (-1;1, -1;1)
  float xpos = (float(position.x) / 512) - 1.0;
  // VRAM puts 0 at the top, OpenGL at the bottom, we must mirror
  // vertically
  float ypos = 1.0 - (float(position.y) / 256);

  gl_Position.xyzw = vec4(xpos, ypos, 0.0, 1.0);

  color = vertex_color;
}
