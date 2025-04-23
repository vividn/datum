import colornames from "colornames";
import { md5Color } from "../utils/md5Color";

/**
 * Convert a color name to a hex color
 * Returns the hex code if valid, or null if not found
 */
export function convertNameToColor(name: string): string | null {
  if (name.startsWith("#")) {
    // It's already a hex color, validate it
    if (/^#([0-9A-Fa-f]{6}|[0-9A-Fa-f]{3})$/.test(name)) {
      // Convert 3-digit hex to 6-digit if needed
      if (name.length === 4) {
        const r = name[1];
        const g = name[2];
        const b = name[3];
        return `#${r}${r}${g}${g}${b}${b}`;
      }
      return name;
    }
    return null;
  }
  
  // Try to find the color by name
  const hex = colornames(name);
  if (hex) {
    return hex;
  }
  
  // If no color found, return null
  return null;
}

/**
 * Generate a random color in a specified hue range
 */
export function generateRandomColor(
  hue?: [number, number],
  saturation: [number, number] = [60, 100],
  lightness: [number, number] = [30, 70]
): string {
  // If no hue range is provided, use a random hue
  const h = hue
    ? Math.floor(Math.random() * (hue[1] - hue[0]) + hue[0])
    : Math.floor(Math.random() * 360);
    
  // Random saturation and lightness within the specified ranges
  const s = Math.floor(Math.random() * (saturation[1] - saturation[0]) + saturation[0]);
  const l = Math.floor(Math.random() * (lightness[1] - lightness[0]) + lightness[0]);
  
  // Convert HSL to hex
  return hslToHex(h, s, l);
}

/**
 * Generate color variations based on a base color
 * @param baseColor - The base color to generate variations from (hex or name)
 * @param count - The number of variations to generate
 * @returns Array of hex color strings
 */
export function generateColorVariations(
  baseColor: string,
  count: number = 4
): string[] {
  let hex = convertNameToColor(baseColor);
  
  // If no valid color is found, use md5 hash of the name to generate one
  if (!hex) {
    hex = md5Color(baseColor);
  }
  
  // Convert hex to HSL
  const hsl = hexToHSL(hex);
  if (!hsl) {
    // Fallback if conversion fails
    return Array(count).fill(0).map(() => generateRandomColor());
  }
  
  const [h, s, l] = hsl;
  const variations: string[] = [];
  
  // Generate variations by varying lightness and saturation
  for (let i = 0; i < count; i++) {
    // Vary each color around the base HSL values
    const newH = (h + (i * 15) % 360) % 360;  // Slightly shift hue
    const newS = Math.max(20, Math.min(100, s + (i % 3 - 1) * 15)); // Vary saturation
    const newL = Math.max(15, Math.min(85, l + ((i + 1) % 3 - 1) * 15)); // Vary lightness
    
    variations.push(hslToHex(newH, newS, newL));
  }
  
  return variations;
}

/**
 * Convert hex color to HSL values
 * @param hex - Hex color string (e.g. "#ff0000")
 * @returns [h, s, l] array or null if invalid
 */
export function hexToHSL(hex: string): [number, number, number] | null {
  // Remove # if present
  hex = hex.replace(/^#/, '');
  
  // Parse RGB values
  let r, g, b;
  if (hex.length === 3) {
    r = parseInt(hex[0] + hex[0], 16) / 255;
    g = parseInt(hex[1] + hex[1], 16) / 255;
    b = parseInt(hex[2] + hex[2], 16) / 255;
  } else if (hex.length === 6) {
    r = parseInt(hex.substring(0, 2), 16) / 255;
    g = parseInt(hex.substring(2, 4), 16) / 255;
    b = parseInt(hex.substring(4, 6), 16) / 255;
  } else {
    return null;
  }
  
  // Find min and max values for RGB
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0, s = 0, l = (max + min) / 2;
  
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    
    switch (max) {
      case r: h = (g - b) / d + (g < b ? 6 : 0); break;
      case g: h = (b - r) / d + 2; break;
      case b: h = (r - g) / d + 4; break;
    }
    
    h = Math.round(h * 60);
  }
  
  s = Math.round(s * 100);
  l = Math.round(l * 100);
  
  return [h, s, l];
}

/**
 * Convert HSL values to hex color
 * @param h - Hue (0-360)
 * @param s - Saturation (0-100)
 * @param l - Lightness (0-100)
 * @returns Hex color string (e.g. "#ff0000")
 */
export function hslToHex(h: number, s: number, l: number): string {
  h /= 360;
  s /= 100;
  l /= 100;
  
  let r, g, b;
  
  if (s === 0) {
    r = g = b = l; // achromatic
  } else {
    const hue2rgb = (p: number, q: number, t: number): number => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1/6) return p + (q - p) * 6 * t;
      if (t < 1/2) return q;
      if (t < 2/3) return p + (q - p) * (2/3 - t) * 6;
      return p;
    };
    
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    
    r = hue2rgb(p, q, h + 1/3);
    g = hue2rgb(p, q, h);
    b = hue2rgb(p, q, h - 1/3);
  }
  
  const toHex = (x: number): string => {
    const hex = Math.round(x * 255).toString(16);
    return hex.length === 1 ? '0' + hex : hex;
  };
  
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}