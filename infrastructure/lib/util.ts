export function toCamelCase(value: string): string {
	return value.replace(/-([a-z])/g, (_, char) => char.toUpperCase());
}
