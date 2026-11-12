# Escrita de registros MARC em ISO 2709 e MARCXML (v0)

> Status: v0, tarefa 1.3. Base: Koha 26.05.05, que escreve MARC pelas bibliotecas Perl `MARC::Record` 2.0.7 (`MARC::File::USMARC`) e `MARC::File::XML`. Cobre a **serialização** de um registro já pronto; quais campos entram e como o texto é normalizado em cada tela de exportação está em "Fora do escopo".

## 1. Comportamento

1. Um registro tem um líder de 24 bytes e uma lista ordenada de campos.
   - Campo de controle: etiqueta `00X` e um valor.
   - Campo de dados: etiqueta, dois indicadores e uma lista ordenada de subcampos (código de um byte e valor).
2. **ISO 2709**:
   - Cada campo é serializado assim:
     - campo de controle: o valor seguido de `0x1E`;
     - campo de dados: os dois indicadores, cada subcampo como `0x1F` + código + valor, e `0x1E` no fim.
   - A entrada do diretório é etiqueta (3) + tamanho do campo com o terminador (4 dígitos) + início relativo à base (5 dígitos), na ordem dos campos.
   - Endereço-base = 24 + 12 × número de campos + 1. Tamanho do registro = base + soma dos campos + 1.
   - O líder recebe o tamanho (posições 0–4) e a base (12–16), e as constantes do MARC21 `22` (10–11) e `4500` (20–23). As outras posições ficam como vieram.
   - Saída: líder, diretório, `0x1E`, campos, `0x1D`.
3. **MARCXML de um registro** (`as_xml_record`, usado pelo OPAC):
   - Cabeçalho completo `<?xml …?>` e `<record>` com os atributos de namespace e `schemaLocation` do MARC21 slim, cada atributo na sua linha, seguido de uma linha em branco.
   - `  <leader>`, `  <controlfield tag="…">`, `  <datafield tag="…" ind1="…" ind2="…">` e `    <subfield code="…">`, uma linha por elemento, com `\n` entre elas.
   - Termina em `</record>\n`.
4. **MARCXML em coleção** (exportação da equipe):
   - O arquivo começa com `<?xml version="1.0" encoding="UTF-8"?>` e `<collection>` com os mesmos atributos (indentados com dois espaços), seguido de uma linha em branco.
   - Cada registro vem como no item 3, mas abre com `<record>` sem atributos e é seguido de uma linha em branco.
   - O arquivo fecha com `</collection>` e uma quebra de linha.
5. Escape no XML: `&`, `<`, `>`, `'` e `"` viram `&amp;`, `&lt;`, `&gt;`, `&apos;` e `&quot;`, no texto, nos indicadores, nas etiquetas e nos códigos. Nada mais é escapado.
6. O MARCXML exige registro em UTF-8 (líder 9 = `a`). A conversão de MARC-8 é da tarefa 1.4.
7. **Publicação atômica:**
   - O arquivo exportado é escrito num temporário na mesma pasta, gravado em disco e só então renomeado para o nome final.
   - Quem lê o arquivo final vê a versão anterior inteira ou a nova inteira, nunca um arquivo pela metade.
   - Se a escrita falha, o temporário é apagado e o arquivo anterior fica intacto.

## 2. Origem no Koha

- `MARC::File::USMARC::encode` e `_build_tag_directory`, `MARC::Record::set_leader_lengths`, `MARC::Field::as_usmarc`: <https://github.com/perl4lib/marc-perl>, `marc-record/lib/MARC/`.
- `MARC::File::XML::record`, `header`, `footer`, `escape` e `encode`: mesmo repositório, `marc-xml/lib/MARC/File/XML.pm`.
- `reference/koha/opac/opac-export.pl`, formatos `utf8` e `marcxml`.
- `reference/koha/Koha/Exporter/Record.pm` (`export`), formatos `iso2709` e `xml`.
- `reference/koha/C4/Record.pm`: `marc2marcxml` e `_entity_encode`.

## 3. Paridade

`exigida` (exportação MARC, doc 04 §2). Prova: `crates/atrium-marc/tests/koha_export.rs`, com o registro 1 exportado pelo OPAC do oráculo em 2026-11-12 nos formatos `utf8` (ISO 2709) e `marcxml`, em `crates/atrium-marc/tests/data/`.

- O ISO 2709 lido e reescrito sai igual byte a byte.
- O MARCXML gerado pelo Atrium, depois de passar pela mesma normalização NFC e pelas mesmas entidades `&#x…;` que o `C4::Record::_entity_encode` aplica, é igual byte a byte ao do Koha.

## 4. Casos de borda

1. Registro sem campos: base 25, tamanho 26.
2. Campo de dados sem subfields: só indicadores e terminador.
3. Campo com mais de 9.999 bytes, ou registro com mais de 99.999: o Atrium recusa o registro com erro.
   - O `MARC::Record` escreveria um registro inválido (tamanho truncado em 99999, ou uma entrada de diretório com 5 dígitos no lugar de 4).
   - A exportação da equipe do Koha relê cada registro e pula o que tem avisos, então recusar dá o mesmo resultado nela.
4. Valor com `0x1D`, `0x1E` ou `0x1F`, código de subcampo igual a um desses bytes, ou indicador que seja um deles: recusado pelo mesmo motivo do item 3.
5. Registro lido com bytes entre os indicadores e o primeiro `0x1F`, ou com subcampo vazio sem código: esses bytes se perdem ao reescrever, como no `MARC::Record`.
6. Campo de dados lido com menos de dois bytes: os indicadores que faltam viram espaço.
7. Caracteres de controle (abaixo de `0x20`, fora `\t`, `\n` e `\r`) num valor saem crus no XML, como no Koha. O resultado não é XML válido; registrado aqui para um desvio futuro, se a biblioteca quiser.
8. Publicação atômica com a pasta de destino inexistente: erro, sem temporário perdido.

## 5. Desvios

Nenhum. Os itens 3 e 4 do §4 seguem a exportação da equipe; o OPAC do Koha entregaria o registro corrompido.

## 6. Fora do escopo

- Escolha de campos e itens embutidos (`952`), filtros de visibilidade do OPAC e de campos ocultos: tarefas 1.6 e 1.11.
- Normalização Unicode na exportação: o formato `utf8` do OPAC converte os campos de dados para NFD (`C4::Charset::SetUTF8Flag`), e o `marcxml` do OPAC aplica NFC e entidades `&#x…;`. Ficam para a tarefa 1.11, junto com as telas e a linha de comando de exportação.
- MARC-8 e UNIMARC no XML (`encoding` ISO-8859-1 do UNIMARC): tarefas 1.4 e Fase 4.
