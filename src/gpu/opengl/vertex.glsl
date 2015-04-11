#version 330 core

in ivec2 vertex_position;
in uvec3 vertex_color;

out vec3 color;

void main() {
  // Convert VRAM coordinates (0;1023, 0;511) into OpenGL coordinates
  // (-1;1, -1;1)
  float xpos = (float(vertex_position.x) / 512) - 1.0;
  // VRAM puts 0 at the top, OpenGL at the bottom, we must mirror
  // vertically
  float ypos = 1.0 - (float(vertex_position.y) / 256);

  gl_Position.xyzw = vec4(xpos, ypos, 0.0, 1.0);

  // Convert the components from [0;255] to [0;1]
  color = vec3(float(vertex_color.r) / 255,
	       float(vertex_color.g) / 255,
	       float(vertex_color.b) / 255);
}
