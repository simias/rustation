#version 330 core

// We're sampling from the internal framebuffer texture
uniform sampler2D fb;
uniform float alpha;
// Framebuffer sampling: 0: Normal 16bpp mode, 1: Use 24bpp mode
uniform int depth_24bpp;

in vec2 frag_fb_coord;

out vec4 frag_color;

// Take a normalized color and convert it into a 16bit 1555 ABGR
// integer in the format used internally by the Playstation GPU.
int rebuild_color(vec4 color) {
  int a = int(floor(color.a + 0.5));
  int r = int(floor(color.r * 31. + 0.5));
  int g = int(floor(color.g * 31. + 0.5));
  int b = int(floor(color.b * 31. + 0.5));

  return (a << 15) | (b << 10) | (g << 5) | r;
}

void main() {
  vec3 color;

  if (depth_24bpp == 0) {
    // Use the regular 16bpp mode, fetch directly from the framebuffer
    // texture. The alpha/mask bit is ignored here.
    color = texture(fb, frag_fb_coord).rgb;
  } else {
    // In this mode we have to interpret the framebuffer as containing
    // 24bit RGB values instead of the usual 16bits 1555.

    ivec2 fb_size = textureSize(fb, 0);

    int x_24 = int(frag_fb_coord.x * float(fb_size.x));
    int y = int((frag_fb_coord.y * float(fb_size.y)));

    // The 24bit color is stored over two 16bit pixels, convert the
    // coordinates
    int x_16 = (x_24 * 3) / 2;

    int col0 = rebuild_color(texelFetch(fb, ivec2(x_16, y), 0));
    int col1 = rebuild_color(texelFetch(fb, ivec2(x_16 + 1, y), 0));

    int col = (col1 << 16) | col0;

    // If we're drawing an odd 24 bit pixel we're starting in the
    // middle of a 16bit cell so we need to adjust accordingly.
    col >>= 8 * (x_24 & 1);

    // Finally we can extract and normalize the 24bit pixel
    float b = float((col >> 16) & 0xff) / 255.;
    float g = float((col >> 8) & 0xff) / 255.;
    float r = float(col & 0xff) / 255.;

    color = vec3(r, g, b);
  }

  frag_color = vec4(color, alpha);
}
