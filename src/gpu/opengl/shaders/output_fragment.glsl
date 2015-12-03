#version 330 core

// We're sampling from the internal framebuffer texture
uniform sampler2D fb;
uniform float alpha;

in vec2 frag_fb_coord;

out vec4 frag_color;

void main() {
  frag_color = vec4(texture(fb, frag_fb_coord).rgb, alpha);
}
