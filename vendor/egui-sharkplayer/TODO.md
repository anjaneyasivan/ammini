## Bugs

- Remove any remaining framebuffer on video load. Otherwise, when switching
  videos, the last frame of the preivous video remains until the new video starts
  rendering.
