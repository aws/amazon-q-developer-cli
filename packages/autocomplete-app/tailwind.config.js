/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    keyframes: {
      spinGrow: {
        "0%": { transform: "scale(0)" },
        "100%": { transform: "scale(1)" },
      },
      spinShrink: {
        "0%": { transform: "scale(1)" },
        "100%": { transform: "scale(0)" },
      },
      spinSlide: {
        "0%": { transform: "translateX(0)" },
        "100%": { transform: "translateX(14px)" },
      },
    },
    extend: {
      colors: {
        // colors from v1 (8 colors, component focused)
        "main-bg": "rgb(var(--main-bg-color, 48 48 48) / <alpha-value>)",
        "main-text": "rgb(var(--main-text-color, 180 180 180) / <alpha-value>)",
        "description-text":
          "rgb(var(--description-text-color, 180 180 180) / <alpha-value>)",
        "description-border":
          "rgb(var(--description-border-color, 65 65 65) / <alpha-value>)",
        "selected-bg":
          "rgb(var(--selected-bg-color, 30 90 199) / <alpha-value>)",
        "selected-text":
          "rgb(var(--selected-text-color, 253 253 253) / <alpha-value>)",
        "matching-bg":
          "rgb(var(--matching-bg-color, 95 89 56) / <alpha-value>)",
        "selected-matching-bg":
          "rgb(var(--selected-matching-bg-color, 106 142 218) / <alpha-value>)",

        // new colors (16 colors, general purpose)
        // the background color of the page
        shade0: "rgb(var(--shade0-color, 45 45 45) / <alpha-value>)",
        shade1: "rgb(var(--shade1-color, 57 57 57) / <alpha-value>)",
        shade2: "rgb(var(--shade2-color, 81 81 81) / <alpha-value>)",
        shade3: "rgb(var(--shade3-color, 119 119 119) / <alpha-value>)",
        shade4: "rgb(var(--shade4-color, 180 183 180) / <alpha-value>)",
        shade5: "rgb(var(--shade5-color, 204 204 204) / <alpha-value>)",
        shade6: "rgb(var(--shade6-color, 224 224 224) / <alpha-value>)",
        // the "brightest" color, used for text
        shade7: "rgb(var(--shade7-color, 255 255 255) / <alpha-value>)",
        // Typically Red
        accent0: "rgb(var(--accent0-color, 210 82 82) / <alpha-value>)",
        // Typically Orange
        accent1: "rgb(var(--accent1-color, 249 169 89) / <alpha-value>)",
        // Typically Yellow
        accent2: "rgb(var(--accent2-color, 255 198 109) / <alpha-value>)",
        // Typically Green
        accent3: "rgb(var(--accent3-color, 165 194 97) / <alpha-value>)",
        // Typically Light Blue
        accent4: "rgb(var(--accent4-color, 190 214 255) / <alpha-value>)",
        // Typically Dark Blue
        accent5: "rgb(var(--accent5-color, 108 153 187) / <alpha-value>)",
        // Typically Purple
        accent6: "rgb(var(--accent6-color, 209 151 217) / <alpha-value>)",
        // Varies,
        accent7: "rgb(var(--accent7-color, 249 115 148) / <alpha-value>)",
      },
    },
  },
  plugins: [],
};
