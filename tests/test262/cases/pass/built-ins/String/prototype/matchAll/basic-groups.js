const matches = Array.from('ab12cd34'.matchAll(/(?<letters>[a-z]+)(\d+)/g));

matches.map((match) => [match[0], match[1], match[2], match.index, match.groups.letters]);
