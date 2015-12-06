#version 330 core

uniform sampler2D fb_texture;

in vec4 frag_shading_color;
flat in uvec2 frag_texture_page;
in vec2 frag_texture_coord;
flat in uvec2 frag_clut;
flat in float frag_texture_blend;

out vec4 frag_color;

int rebuild_color(vec4 color) {
  int a = int(round(color.a));
  int r = int(round(color.r * 31.));
  int g = int(round(color.g * 31.));
  int b = int(round(color.b * 31.));

  return((a << 15) | (b << 10) | (g << 5) | r);
}

void main() {
  int res = 4;

  float tex_x_float = frag_texture_coord.x / float(res);

  int align = int(fract(tex_x_float) * res);

  int tex_x = int(frag_texture_page.x) + int(tex_x_float);
  int tex_y = int(frag_texture_page.y) + int(frag_texture_coord.y);

  vec4 texel = texelFetch(fb_texture, ivec2(tex_x, 511 - tex_y), 0);

  // Recompose 1555 the color
  int icolor = rebuild_color(texel);

  int clut_idx = (icolor >> (align * (16 / res))) & 0xf;

  int clut_x = int(frag_clut.x) + clut_idx;
  int clut_y = int(frag_clut.y);

  //int clut_x = 320 + clut_idx;
  //int clut_y = 480;

  vec4 tex_color = texelFetch(fb_texture, ivec2(clut_x, 511 - clut_y), 0);
  //float comp = float(clut_idx) / 0xff;

  //vec4 tex_color = vec4(texel.rgb, 1.0);

  // Blend the texture color and shading color acconding to the
  // blending mode
  vec4 end_col =
    frag_shading_color * (1.0 - frag_texture_blend) +
    tex_color * frag_texture_blend;

  frag_color = vec4(end_col.rgb, 1.0);
}
