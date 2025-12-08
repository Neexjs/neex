import gradient from 'gradient-string';

export function displayNeexLogo(version: string) {
  const logoText = [
    '███╗   ██╗███████╗███████╗██╗  ██╗',
    '████╗  ██║██╔════╝██╔════╝╚██╗██╔╝',
    '██╔██╗ ██║█████╗  █████╗   ╚███╔╝ ',
    '██║╚██╗██║██╔══╝  ██╔══╝   ██╔██╗ ',
    '██║ ╚████║███████╗███████╗██╔╝ ██╗',
    '╚═╝  ╚═══╝╚══════╝╚══════╝╚═╝  ╚═╝',
    `                    v${version}`,
  ];

  const gradientText = logoText
    .map(line => gradient('#0066FF', '#7FB1D3')(line))
    .join('\n');
  console.log(gradientText);
}
