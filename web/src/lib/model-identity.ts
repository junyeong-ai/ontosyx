export function formatModelIdentity(provider: string, model: string): string {
  const cleanProvider = provider.trim();
  const cleanModel = model.trim();
  if (!cleanProvider) return cleanModel;
  if (!cleanModel) return cleanProvider;
  return `${cleanProvider}/${cleanModel}`;
}
