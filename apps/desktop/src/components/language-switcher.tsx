import { useTranslation } from 'react-i18next';

const languages = [
  { code: 'en-US', label: 'English' },
  { code: 'zh-CN', label: '简体中文' },
] as const;

export function LanguageSwitcher() {
  const { i18n } = useTranslation();

  const handleChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const lang = e.target.value;
    i18n.changeLanguage(lang);
    localStorage.setItem('latexlabs-lang', lang);
  };

  return (
    <select
      value={i18n.language}
      onChange={handleChange}
      className="rounded border border-border bg-background px-2 py-1 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
      aria-label="Select language"
    >
      {languages.map((lang) => (
        <option key={lang.code} value={lang.code}>
          {lang.label}
        </option>
      ))}
    </select>
  );
}
