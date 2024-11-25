type MessageType = "info" | "warning" | "error";

/**
 * Calculates perceived brightness of a color using YIQ formula
 */
function getColorBrightness(hexColor: string): number {
  const hex = hexColor.replace("#", "");
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  return (r * 299 + g * 587 + b * 114) / 1000;
}

/**
 * Returns a readable text color based on background color and message type
 * @param backgroundColor - Hex color string (e.g. '#ffaa00')
 * @param messageType - Optional type of message ('info', 'warning', 'error')
 * @returns Appropriate text color as hex string
 */
export function getContrastTextColor(
  backgroundColor: string,
  messageType?: MessageType,
): string {
  const brightness = getColorBrightness(backgroundColor);
  const isBright = brightness > 128;

  if (!messageType) {
    return isBright ? "#000000" : "#ffffff";
  }

  // Get background RGB values for comparison
  const bgHex = backgroundColor.replace("#", "");
  const bgR = parseInt(bgHex.substring(0, 2), 16);
  const bgG = parseInt(bgHex.substring(2, 4), 16);
  const _bgB = parseInt(bgHex.substring(4, 6), 16);

  switch (messageType) {
    case "warning":
      // Use darker yellow if background is bright or yellowish
      if (bgR > 200 && bgG > 200) {
        return "#bB8000"; // Yellow orange
      }
      return "#FFD700"; // Bright yellow

    case "error":
      // Use darker red if background is bright or reddish
      if (bgR > 200) {
        return "#8B0000"; // Dark red
      }
      return "#FF0000"; // Bright red

    case "info":
    default:
      return isBright ? "#000000" : "#ffffff";
  }
}
