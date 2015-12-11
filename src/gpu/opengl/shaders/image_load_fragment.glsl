#version 330 core

uniform sampler2D image;

in vec2 frag_image_coord;

out vec4 frag_color;

void main() {
  // The interpolation *must* be set to nearest! We can't filter
  // textures here because they could be paletted
  frag_color = texelFetch(image, ivec2(frag_image_coord), 0);
}
