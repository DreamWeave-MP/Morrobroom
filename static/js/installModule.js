const DefaultInstallPath = "C:/Games/OpenMW-Mods/";
const pathRegex = new RegExp(DefaultInstallPath.replace(/\\/g, '\\\\'), 'g');
const pathSep = '/';

function normalizePath(path) {
    if (!path) return DefaultInstallPath;

    path = path.trim();

    let cleaned = path.replace(/\//g, pathSep).replace(/\\/g, pathSep).replace(/\\ /g, ' ')
    if (cleaned.endsWith(pathSep)) {
        cleaned = cleaned.slice(0, -1);
    }

    return cleaned
}

function loadModInstallPath() {
    const cachedPath = localStorage.getItem('modInstallPath');
    const configSection = document.getElementById('config-code');
    const installInput = document.getElementById('install-path-input');

    if (!cachedPath || !configSection || !installInput) {
        return;
    }

    let normalized = normalizePath(
        configSection.textContent.replace(pathRegex, cachedPath)
    ).replace(/\\/g, pathSep);

    if (normalized.endsWith(pathSep)) {
        normalized = normalized.slice(0, -1);
    }

    configSection.textContent = normalized;

    installInput.placeholder = cachedPath;
}

window.addEventListener('DOMContentLoaded', loadModInstallPath)

function generateConfig() {
    const modData = document.getElementById('mod-data');
    if (!modData) {
        alert('Mod data not found. Please ensure the webpage is loaded correctly.');
        return;
    }

    let configSection = document.getElementById('config-code');
    if (!configSection) {
        alert('Config section not found. Please ensure the webpage is loaded correctly.');
        return;
    }

    let inputPath = document.getElementById('install-path-input').value.trim();
    const userInputPath = inputPath !== '';

    inputPath = normalizePath(inputPath);
    console.debug(`normalized: ${inputPath}`);

    if (userInputPath) {
        console.debug(`Caching path ${inputPath}`);
        localStorage.setItem('modInstallPath', inputPath);
    }

    const pathSep = inputPath.includes('\\') ? '\\' : '/';
    const trimmedName = modData.getAttribute("name").trim()

    const fallbackEntries = JSON.parse(modData.getAttribute('fallback_entries'));
    const fallbackEntryHeaderString = `# ${trimmedName} Fallback Entries\n`;

    let fallbackEntryString = fallbackEntries ? '' : null;
    if (fallbackEntries) {
        for (setting in fallbackEntries) {
            fallbackEntryString = `${fallbackEntryString}${setting}=${fallbackEntries[setting]}\n`;
        }
    }

    const contentFiles = JSON.parse(modData.getAttribute('content_files'));
    const contentFileHeaderString = `# ${trimmedName} Content Files\n`;
    const contentFileString = contentFiles ? contentFiles.map(file => {
        return `content=${file.trim()}`;
    }
    ).join('\n') : null;

    const dataDirectories = JSON.parse(modData.getAttribute('data_directories'));
    const modTitle = modData.getAttribute('name');
    const dataDirectoryHeaderString = `# ${trimmedName} Data Directories\n`;
    const dataDirectoryString = dataDirectories ? dataDirectories.map(path => {
        return `data=${inputPath}${modTitle}/${path.replace('\\', '/')}`;
    }
    ).join('\n') : null;

    console.debug(dataDirectoryHeaderString, dataDirectoryString, contentFileHeaderString, contentFileString, fallbackEntryHeaderString, fallbackEntryString);
    let outputText = '';

    if (fallbackEntryString) {
      outputText += `${fallbackEntryHeaderString}${fallbackEntryString}\n`
    }
    
    outputText += `${dataDirectoryHeaderString}${dataDirectoryString}\n`;

    if (contentFileString) {
        outputText += `\n${contentFileHeaderString}${contentFileString}\n`
    }

    configSection.textContent = outputText;
}

function unsecuredCopyToClipboard(text) { 
  const textArea = document.createElement("textarea");
  textArea.value = text;
  document.getElementById('install-info-outer').appendChild(textArea);
  textArea.focus();
  textArea.select();
  try {
    document.execCommand('copy')
  } catch (err) {
    console.error('Unable to copy to clipboard', err)
  }
  document.getElementById('install-info-outer').removeChild(textArea)
};

function copyConfigToClipboard() {
    const configSection = document.getElementById('config-code');

    if (window.isSecureContext && navigator.clipboard) {
        navigator.clipboard.writeText(configSection.textContent);
    } else {
        unsecuredCopyToClipboard(configSection.textContent);
    }
}
