/** Detect whether a string looks like a Git URL (as opposed to a local path). */
export function isGitUrl(value: string): boolean {
  return (
    value.startsWith("https://") ||
    value.startsWith("http://") ||
    value.startsWith("git://") ||
    value.startsWith("ssh://") ||
    /^[\w.-]+@[\w.-]+:/.test(value) // git@host:org/repo
  );
}
