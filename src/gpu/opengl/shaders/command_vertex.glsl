#version 330 core

// Vertex shader for rendering GPU draw commands in the framebuffer

in ivec2 position;
in vec3 color;
in float alpha;

// Drawing offset
uniform ivec2 offset;

out vec4 v_color;

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
  v_color = vec4(color / 255, alpha);
}
