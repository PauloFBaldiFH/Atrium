# Conversão de MARC-8 e ISO 5426 para UTF-8 (v0)

> Status: v0, tarefa 1.4. Base: Koha 26.05.05 (`C4::Charset`) e a biblioteca Perl `MARC::Charset` 1.35, que o Koha usa para MARC-8 (ver §6 sobre a versão). Cobre a conversão de texto e a decisão de codificação de um registro MARC21 na importação.

## 1. Comportamento

### Registro MARC21 de codificação desconhecida (`MarcToUTF8Record`)

1. Se todos os valores do registro são UTF-8 válido, o registro é tratado como UTF-8. A posição 9 do líder não é consultada, porque muitos sistemas a preenchem errado.
2. Senão, cada valor de subcampo é convertido de MARC-8. Se algum byte não tiver correspondência em nenhum subcampo, a tentativa inteira é descartada e o registro todo é convertido de ISO-8859-1 (cada byte vira o caractere Unicode de mesmo número).
3. Campos de controle e indicadores não são convertidos.
4. O líder sai com a posição 9 = `a`. O resultado informa qual codificação foi usada: UTF-8, MARC-8 ou ISO-8859-1.

### Texto em MARC-8 (`MARC::Charset::marc8_to_utf8`)

5. Cada valor começa com G0 = ASCII básico (`B`) e G1 = latim estendido, ANSEL (`E`).
6. Espaço, `\t`, `\n`, `\v`, `\f` e `\r` passam como estão.
7. Sequências de escape:
   - `ESC g`, `ESC b`, `ESC p` e `ESC s` trocam o G0 para símbolos gregos, subscritos, sobrescritos ou ASCII;
   - `ESC (` ou `ESC ,` + F trocam o G0 para F; `ESC )` ou `ESC -` + F trocam o G1 para F;
   - `ESC $` + F troca o **G0** para F, mesmo quando F é `)` ou `-`. É o comportamento da `MARC::Charset`: as formas `ESC $ ,`, `ESC $ )` e `ESC $ -` da norma nunca são reconhecidas como tais, e o byte seguinte é lido como caractere;
   - um `ESC` que não forma sequência é lido como caractere (U+001B).
8. Busca do caractere, primeiro no G0 e depois no G1:
   - no conjunto CJK (EACC), cada caractere ocupa 3 bytes;
   - `0x21`–`0x7E` são buscados no G0, e `0xA1`–`0xFE` no G1 (o byte menos `0x80`);
   - `0x88`–`0x8E` são buscados no latim estendido, e `0x1B`–`0x1F` no ASCII básico.
9. Diacríticos (caracteres combinantes) vêm antes da letra no MARC-8 e depois dela no Unicode: ficam guardados e saem logo depois do próximo caractere que não é combinante.
10. As segundas metades da ligadura (`0xEC`) e do til duplo (`0xFB`) são descartadas, porque a primeira metade já vira o diacrítico duplo do Unicode.
11. Um byte sem correspondência é pulado e registrado como erro.
12. Tabelas: as da Library of Congress (`data/loc/marc8.json`), mais seis caracteres EACC não padronizados que a `MARC::Charset` acrescenta, usados pelo sistema III (reticências, aspas curvas, travessão, marca registrada).

### Texto em ISO 5426 (`C4::Charset::char_decode5426`)

13. Bytes de `0x00` a `0x7F` passam como estão, menos `0x00`, `0x01` e `0x0F`, que são removidos.
14. De `0xC0` a `0xDF` (diacríticos): se o par com o byte seguinte está na tabela do Koha, vira esse caractere e consome os dois bytes; senão, vale a tabela para o byte sozinho; senão, o byte vira o caractere Unicode de mesmo número.
15. Os outros bytes (`0x80`–`0xBF`, `0xE0`–`0xFF`): tabela para o byte sozinho, ou o caractere Unicode de mesmo número.
16. Depois, só na **primeira** ocorrência de cada uma, nesta ordem: remove `ESC` seguido de qualquer caractere que não seja `\n`, remove `0x0E` e remove `ESC [`.

## 2. Origem no Koha

- `reference/koha/C4/Charset.pm`:
  - `MarcToUTF8Record` e `_default_marc21_charconv_to_utf8`;
  - `_marc_marc8_to_utf8` e `_marc_to_utf8_via_text_iconv`;
  - `char_decode5426` e a tabela `%chars`, já extraída em `data/koha-26.05/iso5426.json`;
  - `IsStringUTF8ish` e `SetMarcUnicodeFlag`.
- `MARC::Charset` (<https://github.com/perl4lib/marc-perl>, `marc-charset/`):
  - `marc8_to_utf8` e `_process_escape` em `lib/MARC/Charset.pm`;
  - `lib/MARC/Charset/Compiler.pm`: deslocamento G1→G0 dos conjuntos `45`, `51` e `34`, `alt` quando falta `ucs`, metades de diacríticos duplos;
  - `etc/codetables.xml`: mesmos dados da LC, mais `marc_left_half` e `marc_right_half`;
  - `etc/additional-iii-characters.xml`.

## 3. Paridade

`exigida` (importação de registros que depois são exportados e trocados por Z39.50). Prova nesta versão:

- Testes de unidade com os casos documentados nos testes da `MARC::Charset`, como a ligadura e o til duplo.
- Testes de unidade com cada peculiaridade listada acima.

O cenário diferencial com um corpus de registros legados convertidos pelo oráculo fica para quando a importação existir (tarefa 1.7). A importação em lote do Koha exige login de equipe.

## 4. Casos de borda

1. Diacríticos no fim do valor, sem letra depois: são **perdidos**, como na `MARC::Charset`.
2. Diacrítico seguido de espaço: o espaço sai primeiro e o diacrítico vai para a próxima letra.
3. Segunda metade de ligadura sem a primeira: descartada mesmo assim.
4. CJK com menos de 3 bytes restantes: não encontra correspondência e tenta o G1 com 1 byte.
5. Erro de MARC-8 em qualquer subcampo leva o registro **inteiro** para ISO-8859-1, mesmo os subcampos que converteram bem.
6. ISO 5426 com diacrítico no último byte: busca o byte sozinho.
7. ISO 5426: um segundo `ESC x` ou `0x0E` no mesmo valor permanece no texto.
8. Campo de controle com bytes não ASCII num registro MARC-8: fica como está. O registro resultante pode não ser UTF-8 válido nesses campos, e a escrita MARCXML o recusa.
9. Texto só com ASCII imprimível e espaços: devolvido sem cópia (`Cow::Borrowed`).

## 5. Desvios

Nenhum.

## 6. Fora do escopo

- **Versão da `MARC::Charset`.** A versão 1.36 (outubro de 2026) acrescenta um caractere CCCII (`0x212024` → U+FF65) e corrige uma falha com `%` no texto. Esta spec segue a 1.35, a anterior, sem esse caractere; falta confirmar a versão instalada no oráculo (`dpkg -l libmarc-charset-perl`).
- Registro com líder 9 = `a` cujos bytes não são UTF-8 válido: o `MARC::File::USMARC` do Koha decodifica antes da conversão, e esse caminho não foi verificado.
- Conversão de registros UNIMARC (ISO 5426 em nível de registro e marcação no `100$a`): Fase 4. A conversão de texto ISO 5426 já está aqui.
- Normalização NFC/NFD depois da conversão: exportação (1.11) e importação (1.7).
- Codificações escolhidas pelo usuário (`$source_encoding` via `Text::Iconv`).
