/** @type {import('tailwindcss').Config} */
module.exports = {
    content: [
      "./index.html",
      "./src/**/*.{js,ts,jsx,tsx}",
      "./node_modules/@mantine/core/**/*.js",
      "./node_modules/@mantine/notifications/**/*.js",
    ],
    theme: {
      extend: {},
    },
    plugins: [],
  };
  