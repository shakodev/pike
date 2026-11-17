use super::{Document, Error, Field, MAX_PROFILES};

pub(super) fn document(doc: &Document) -> Result<(), Error> {
    let invalid = |field| Error::Invalid(field);
    if doc.profiles.is_empty() || doc.profiles.len() > MAX_PROFILES {
        return Err(invalid(Field::Profiles));
    }
    for id in doc
        .profiles
        .keys()
        .chain(std::iter::once(&doc.default_profile))
    {
        if id.0.is_empty()
            || id.0.len() > 32
            || !id
                .0
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(invalid(Field::ProfileId));
        }
    }
    if !doc.profiles.contains_key(&doc.default_profile) {
        return Err(invalid(Field::DefaultProfile));
    }
    if !doc.font.size.is_finite() || !(10.0..=28.0).contains(&doc.font.size) {
        return Err(invalid(Field::FontSize));
    }
    if doc.history > 10_000 {
        return Err(invalid(Field::History));
    }
    for profile in doc.profiles.values() {
        if let Some(shell) = &profile.shell {
            let path = std::path::Path::new(shell);
            // Bare names use the adapter's existing PATH lookup. Paths are
            // absolute, so reload/new launches cannot reinterpret relative cwd.
            let path_like = shell.contains(['/', '\\', ':']);
            if shell.is_empty()
                || shell.len() > 4096
                || shell.chars().any(char::is_control)
                || shell.contains('"')
                || (path_like && (!path.is_absolute() || !path.is_file()))
            {
                return Err(invalid(Field::Shell));
            }
        }
        if (profile.shell.is_none() && !profile.args.is_empty())
            || profile.args.len() > 64
            || profile
                .args
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
            || profile.args.iter().map(String::len).sum::<usize>() > 16_384
        {
            return Err(invalid(Field::Args));
        }
        if let Some(cwd) = &profile.cwd {
            let text = cwd.to_string_lossy();
            if !cwd.is_absolute()
                || !cwd.is_dir()
                || text.len() > 4096
                || text.chars().any(char::is_control)
            {
                return Err(invalid(Field::Cwd));
            }
        }
        let mut names = std::collections::BTreeSet::new();
        if profile.env.len() > 64
            || profile.env.iter().any(|(key, value)| {
                key.is_empty()
                    || !key.is_ascii()
                    || key.len() > 256
                    || key.contains(['=', '\0'])
                    || key.chars().any(char::is_control)
                    || value.len() > 4096
                    || value.contains('\0')
                    || !names.insert(key.to_ascii_uppercase())
            })
            || profile
                .env
                .iter()
                .map(|(key, value)| key.len() + value.len() + 2)
                .sum::<usize>()
                > 16_384
        {
            return Err(invalid(Field::Environment));
        }
    }
    super::shortcuts::validate(&doc.shortcuts)
}
