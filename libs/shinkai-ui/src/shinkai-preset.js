/** @type {import('tailwindcss').Config} */
const tailwindTypography = require('@tailwindcss/typography');
const defaultTheme = require('tailwindcss/defaultTheme');
const plugin = require('tailwindcss/plugin');
const tailwindAnimate = require('tailwindcss-animate');

module.exports = {
  content: [],
  darkMode: ['class'],
  theme: {
    container: {
      center: true,
      padding: '1.5rem',
      screens: {
        '2xl': '896px',
      },
    },
    extend: {
      fontSize: {
        'em-xs': '0.75em',
        'em-sm': '0.875em',
        'em-base': '1em',
        'em-lg': '1.125em',
        'em-xl': '1.25em',
        'em-2xl': '1.5em',
      },
      height: {
        input: '59px',
      },
      colors: {
        brand: {
          DEFAULT: 'hsla(360, 99%, 69%, 1)',
          500: 'hsla(359, 58%, 58%, 1)',
        },

        official: {
          gray: {
            100: '#f0f0f0',
            200: '#d0d0d0',
            300: '#b0b0b0',
            400: '#B3B3B3',
            500: '#707070',
            600: '#5a5a5a',
            700: '#464648',
            750: '#3c3c3f',
            780: '#313336', //  borders
            800: '#323234',
            850: '#2a2a2d', // dialogs
            900: '#212023', // inputs
            950: '#1a1a1d',
            1000: '#101113',
            1100: '#0c0d0e',
          },
        },
        // TODO: remove gray, use official-gray (gradually)
        gray: {
          80: 'hsla(0, 0%, 69%, 1)',
          100: 'hsla(250, 7%, 51%, 1)',
          200: 'hsla(214, 5%, 25%, 1)',
          300: 'hsla(216, 5%, 20%, 1)',
          350: 'hsla(263, 7%, 24%, 1)',
          375: 'hsl(255, 7.14%, 21.96%)',
          400: 'hsla(260, 7%, 18%, 1)',
          450: 'hsla(228, 7.04%, 13.92%, 1)',
          500: 'hsla(228, 7%, 14%, 1)',
          600: 'hsla(225, 8.33%, 9.41%, 1)',
          700: 'rgba(12, 13, 14, 1)',
        },
        red: {
          DEFAULT: 'hsla(0, 63%, 51%, 1)',
        },
        // TODO: remove tray
        primary: {
          600: '#FF7E7F',
          700: '#FF5E5F',
          800: '#FF3E3F',
        },
        secondary: {
          600: '#19242D',
        },
        foreground: '#FFFFFF',
        'muted-foreground': '#c7c7c7',
      },
      border: {
        input: 'hsl(var(--input))',
      },
      fontFamily: {
        inter: ['Inter', ...defaultTheme.fontFamily.sans],
        clash: ['Clash Display', ...defaultTheme.fontFamily.sans],
      },
      backgroundImage: {
        'brand-gradient':
          'linear-gradient(180deg, rgba(249, 97, 98, 0) 0%, rgba(249, 97, 98, 0.20) 100%)',
        'app-gradient':
          'linear-gradient(90deg, rgba(0, 0, 0, 0.30) 0%, rgba(0, 0, 0, 0.20) 100%)',
        'black-gradient':
          'linear-gradient(94deg, #08090A 21.25%, #121316 81.73%);',
        'onboarding-card':
          'linear-gradient(120deg, #101113 24.72%, #141517 56.36%, #151619 68.07%)',
      },
      keyframes: {
        'accordion-down': {
          from: { height: '0' },
          to: { height: 'var(--radix-accordion-content-height)' },
        },
        'accordion-up': {
          from: { height: 'var(--radix-accordion-content-height)' },
          to: { height: '0' },
        },
        breath: {
          from: { opacity: 1 },
          to: { opacity: 0.8 },
        },
        'big-bounce': {
          '0%, 100%': {
            transform: 'translateY(-75%)',
            'animation-timing-function': 'cubic-bezier(0.8, 0, 1, 1)',
          },
          '50%': {
            transform: 'translateY(0)',
            'animation-timing-function': 'cubic-bezier(0, 0, 0.2, 1)',
          },
        },
        'shadow-pulse': {
          '0%': {
            boxShadow: '0 0 0 0px rgba(0, 0, 0, 0.6)',
          },
          '100%': {
            boxShadow: '0 0 0 10px rgba(0, 0, 0, 0)',
          },
        },
        loaderDots: {
          '0%': {
            opacity: 1,
          },
          '50%, 100%': {
            opacity: 0.15,
          },
        },
      },
      animation: {
        'accordion-down': 'accordion-down 0.2s ease-out',
        'accordion-up': 'accordion-up 0.2s ease-out',
        breath: 'breath 5s ease-out infinite',
        'big-bounce': 'big-bounce 1s infinite',
        'shadow-pulse': 'shadow-pulse 2s infinite',
      },
      zIndex: {
        max: 2147483647,
      },
    },
  },
  plugins: [
    tailwindAnimate,
    tailwindTypography,
    plugin(function ({ matchUtilities, theme }) {
      matchUtilities(
        {
          'translate-z': (value) => ({
            '--tw-translate-z': value,
            transform: ` translate3d(var(--tw-translate-x), var(--tw-translate-y), var(--tw-translate-z)) rotate(var(--tw-rotate)) skewX(var(--tw-skew-x)) skewY(var(--tw-skew-y)) scaleX(var(--tw-scale-x)) scaleY(var(--tw-scale-y))`,
          }), // this is actual CSS
        },
        { values: theme('translate'), supportsNegativeValues: true },
      );
    }),
  ],
};
