import eslint from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
	{
		ignores: [
			'dist/**',
			'erd_viewer/pkg/**',
			'infrastructure/**',
			'target/**',
			'**/*.d.ts',
		],
	},
	eslint.configs.recommended,
	...tseslint.configs.strict,
	...tseslint.configs.stylistic,
	{
		languageOptions: {
			ecmaVersion: 2022,
			sourceType: 'module',
			globals: globals.node,
		},
		rules: {
			'@typescript-eslint/no-explicit-any': 'off',
			'@typescript-eslint/no-use-before-define': ['warn', 'nofunc'],
			'@typescript-eslint/no-unused-vars': ['warn', {
				args: 'after-used',
				argsIgnorePattern: '^_',
				vars: 'all',
				varsIgnorePattern: '^_$',
			}],
			'comma-spacing': 2,
			'eol-last': 2,
			'eqeqeq': [2, 'always', { 'null': 'ignore' }],
			'indent': [2, 'tab', {
				'ignoredNodes': ['TemplateLiteral *'],
				'SwitchCase': 1,
			}],
			'keyword-spacing': 2,
			'linebreak-style': ['error', 'unix'],
			'new-parens': 2,
			'no-debugger': 2,
			'no-dupe-args': 2,
			'no-dupe-keys': 2,
			'no-duplicate-case': 2,
			'no-ex-assign': 2,
			'no-fallthrough': 2,
			'no-invalid-this': 2,
			'no-multiple-empty-lines': [2, { 'max': 1 }],
			'no-multi-spaces': 2,
			'no-new-wrappers': 2,
			'no-trailing-spaces': 2,
			'no-undef': 2,
			'no-unneeded-ternary': 2,
			'no-unreachable': 2,
			'no-unused-vars': 'off',
			'no-use-before-define': 'off',
			'no-var': 2,
			'object-curly-spacing': [2, 'always'],
			'prefer-const': 2,
			'quotes': [2, 'single', 'avoid-escape'],
			'semi': 2,
			'space-before-blocks': [2, 'always'],
			'space-before-function-paren': [2, 'never'], //Nr
			'space-in-parens': [2, 'never'],
			'space-infix-ops': 2,
			'strict': [2, 'global'],
			'valid-typeof': 2,
		}
	},
	{
		files: ['erd_app/**'],
		languageOptions: {
			globals: globals.browser,
		}
	},
);
