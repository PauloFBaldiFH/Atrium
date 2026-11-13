# Elzevir: registro de decisões do planejamento

> Status: decisões tomadas por Paulo em 2026-11-12, durante o planejamento. Complementa [01](01-avaliacao-da-arquitetura.md), [02](02-modo-de-implementacao.md) e [03](03-estrategia-reconstrucao.md); onde houver diferença, vale este documento.
> A implementação ainda não começou. Quando começar, cada decisão marcada com **ADR** vira um registro próprio em `docs/adr/`.

## Sumário

1. [Decisões de base](#1-decisões-de-base)
2. [Princípio: o Elzevir não é uma cópia 1 para 1 do Koha](#2-princípio-o-elzevir-não-é-uma-cópia-1-para-1-do-koha)
3. [Koha de referência e oráculo](#3-koha-de-referência-e-oráculo)
4. [Bibliotecas, redes e instituições](#4-bibliotecas-redes-e-instituições)
5. [OPAC e interface da equipe](#5-opac-e-interface-da-equipe)
6. [Plataformas e distribuição](#6-plataformas-e-distribuição)
7. [Painel de gestão no terminal](#7-painel-de-gestão-no-terminal)
8. [Funcionalidades do Koha Nexus](#8-funcionalidades-do-koha-nexus)
9. [Ajustes da revisão externa](#9-ajustes-da-revisão-externa)
10. [Perfil da biblioteca de referência](#10-perfil-da-biblioteca-de-referência)
11. [Em aberto](#11-em-aberto)

---

## 1. Decisões de base

| # | Tema | Decisão | ADR |
|---|---|---|---|
| 1 | Licença | **GPL-3.0-or-later**, a mesma do Koha, para que estudar e reaproveitar o Koha não restrinja o projeto. Arquivo `LICENSE` na raiz; cada arquivo-fonte leva o cabeçalho SPDX `GPL-3.0-or-later`. | sim |
| 2 | Porte | **Biblioteca única e redes de bibliotecas.** Uma base atende uma instituição com uma ou várias unidades (até ~20 unidades e ~2 milhões de itens por base SQLite). Várias instituições independentes podem dividir o mesmo servidor (§4). | sim |
| 3 | Código do Koha | Fica em **`reference/`, fora do workspace Cargo**: nunca é compilado nem empacotado com o Elzevir. Pode virar um repositório separado mais tarde sem mudar nada no resto. | sim |
| 4 | Formatos | **Importação e exportação em MARC21, MARCXML e UNIMARC; a base interna é BIBFRAME 2.0.** MARC21 primeiro (Fase 1), UNIMARC na Fase 4. | sim |
| 5 | Modelo de dados | **Relacional, inspirado em BIBFRAME 2.0** (Obra, Instância, Item, Agente, Assunto em tabelas tipadas), **com o registro MARC original preservado** para exportação sem perdas. Sem triple store RDF; BIBFRAME em RDF só como formato de exportação. | sim |
| 6 | Acesso ao SQLite | **`rusqlite` com ator de escrita única**: uma thread dedicada é dona de todas as escritas (fila limitada, transações em lote), e um pool de conexões atende as leituras em paralelo (WAL). | sim |
| 7 | Idiomas | **Inglês como idioma principal da v1, depois português do Brasil.** Os demais idiomas entram mais tarde. Todo texto de interface passa por Fluent desde o primeiro dia, para que traduzir seja só acrescentar arquivos. | sim |

Consequência do item 7: chaves de mensagem, textos-padrão de avisos e a documentação de uso nascem em inglês; a tradução pt-BR é uma tarefa explícita logo depois da v1 e entra no CI como verificação de chaves faltantes.

## 2. Princípio: o Elzevir não é uma cópia 1 para 1 do Koha

O Horizon reconstrói o Minecraft quase 1 para 1, porque qualquer diferença quebra o jogador. O Elzevir usa **o método** do Horizon (versão fixada, oráculo, livro-razão, extração, camadas), mas **não o objetivo de cópia**:

- **Paridade exigida** onde o comportamento é observável e a biblioteca depende dele: cálculo de datas e calendário, multas, fila de reservas, exportação MARC, SIP2 e Z39.50.
- **Liberdade para melhorar** interface, configuração (`elzevir.toml` tipado no lugar de ~935 preferências do sistema) e busca.
- Toda diferença intencional numa área de paridade fica listada em `docs/desvios-conhecidos.md`.
- Técnicas de desempenho mais elaboradas só entram quando um benchmark pede.

**ADR:** sim (o primeiro, ao começar o código).

## 3. Koha de referência e oráculo

- **Versão-alvo: a estável mais recente, Koha 26.05.05**, a mesma da instância de testes. O código fica em `reference/koha` (submódulo, §1 item 3) e o `koha-lock.json` fixa essa versão.
- **Oráculo:** a instância de testes de Paulo (decisão de 2026-11-12, que substitui o koha-testing-docker no GitHub Actions). Um gravador roda contra ela só quando alguém o chama, com as credenciais em variáveis de ambiente; as saídas ficam gravadas no repositório e o CI compara contra a gravação, sem acessar a instância. Formato e regras em 05, §4.
- **Instância de testes:** cópia descartável e separada da produção, com os leitores removidos (só fica o cadastro de exemplo, que Paulo autorizou usar). Tem o acervo, as regras de circulação e o calendário reais da biblioteca. O backup nunca entra no repositório git, e credenciais nunca vão para documentos ou para o repositório.

## 4. Bibliotecas, redes e instituições

Dois níveis, ambos requisitos desde o início:

| Nível | Como funciona | Limite indicativo |
|---|---|---|
| **Rede dentro de uma instituição** | Uma única base do Elzevir; `branch_id` em tudo que é de unidade (itens, empréstimos, regras, calendários, usuários da equipe); regras e calendários por unidade, como no Koha. | ~20 unidades, ~2 milhões de itens |
| **Várias instituições no mesmo servidor** | Cada instituição tem a sua pasta de dados (base SQLite + índice Tantivy), isolada das outras. O mesmo binário atende todas; a instituição é escolhida pelo endereço (domínio ou subdomínio) da requisição. | definido por máquina |

Cada instituição tem o **seu próprio OPAC e a sua própria interface da equipe**, no seu domínio ou subdomínio. Dentro de uma instituição, cada unidade pode ter a **sua visão do OPAC** (acervo, horários e identidade visual da unidade). Isolamento por pasta também simplifica backup, restauração e remoção de uma instituição.

**ADR:** sim.

## 5. OPAC e interface da equipe

- **Padrão:** mesma porta, separados por caminho: OPAC em `/`, equipe em `/equipe` (o caminho final pode mudar quando a interface em inglês for definida).
- **Configurável** em três modos, oferecidos pelo painel na instalação:
  1. mesma porta e mesmo endereço, separados por caminho (padrão);
  2. separados por nome de host;
  3. equipe numa porta ou interface de rede própria, com lista opcional de IPs permitidos.

```toml
[opac]
listen = "0.0.0.0:443"
host   = "biblioteca.palotina.exemplo"

[equipe]
listen = "192.168.0.10:8443"
host   = "equipe.biblioteca.palotina.exemplo"
permitir_ips = ["192.168.0.0/24"]
```

HTTPS é embutido (sem exigir Nginx, Apache ou IIS), com opção de rodar atrás de um proxy reverso.

## 6. Plataformas e distribuição

**Plataformas de produção, todas de primeira classe:** Linux, macOS, Windows (desktop) e Windows Server, em **x86_64 e ARM64**. Só 64 bits. Mínimos sugeridos: Windows 10, Windows Server 2019, macOS 12.

**Fora do escopo:** BSD e hospedagem em Android. (Android como cliente, por exemplo leitura de código de barras pelo navegador, fica como ideia para depois da v1.)

| Plataforma | Entrega | Serviço |
|---|---|---|
| Linux | Binário estático (musl + mimalloc) que roda em qualquer distribuição; pacotes `.deb`, `.rpm`, `tar.gz` e imagem de container | systemd e OpenRC |
| Windows e Windows Server | `.msi` | Serviço do Windows nativo; regra de firewall; oferta de exclusão do Defender para a pasta de dados; avisos de energia e atualização no desktop |
| macOS | `.pkg` | launchd |

- A base de dados precisa estar em **disco local** (SQLite em WAL não funciona com segurança em compartilhamento de rede); o `init` verifica isso.
- **CI** compila e testa nas três famílias de sistema e nas duas arquiteturas; testes de importação e indexação rodam no Windows desde a Fase 0. Distribuições Linux testadas: Debian/Ubuntu, família RHEL (Rocky/Alma) e Alpine.

**ADR:** sim.

## 7. Painel de gestão no terminal

- Painel simples no terminal, no espírito do painel do Koha Nexus, escrito **em Rust com `ratatui`** e embutido no mesmo binário: `elzevir panel`.
- É só uma interface sobre os mesmos subcomandos de linha de comando, que continuam utilizáveis em scripts: `init`, `status`, `backup`, `update`, `scheduler`, `domain`, usuários administradores, `logs`, `reindex`, `import`.
- Funciona igual nas quatro plataformas, sem Python ou outro interpretador instalado.

## 8. Funcionalidades do Koha Nexus

O Koha Nexus é o painel do Paulo que instala e estende o Koha. As funcionalidades abaixo têm lugar previsto no Elzevir; os detalhes serão discutidos depois, quando o repositório do Nexus for compartilhado.

| Funcionalidade do Nexus | Onde entra no Elzevir |
|---|---|
| Busca Z39.50 avançada em vários servidores | Núcleo, Fase 4. Na Fase 1 o fluxo de catalogação já nasce com um ponto de extensão de **"fontes de rascunho"**, que também servirá para SRU e para catalogação assistida por IA a partir de fotos |
| Personalização visual avançada | *Tokens* de design desde a Fase 0; editor visual depois |
| Subdomínios gratuitos (Cloudflare Worker) | Considerado valioso; combina com o modelo de várias instituições por servidor (§4). Forma de inclusão a discutir |
| Backups na nuvem com rclone | Primeiro o backup nativo do SQLite; destinos rclone depois |
| Lista intuitiva de tarefas agendadas (cron) | Modelo para a tela do agendador interno do Elzevir (Fase 3) |

## 9. Ajustes da revisão externa

Três ajustes aceitos a partir da revisão do Gemini, já aplicados nos docs 01 a 03:

- **A. Leitura MARC sem `unsafe` por padrão.** `elzevir-marc` lê os arquivos em fluxo, com um *buffer* reaproveitado, e o `RecordRef` empresta fatias desse *buffer*. Mapear o arquivo (`memmap2`) só entra se um benchmark mostrar ganho, num único bloco `unsafe` documentado. As crates de domínio, aplicação e web usam `#![forbid(unsafe_code)]`.
- **B. Fase 0 enxuta.** Prioridade: workspace, CI, `elzevir-store` e `elzevir-marc`. O extrator começa só com `$RULE_KINDS`, o livro-razão começa como um script pequeno, e o executor de cenários passa para o início da Fase 1 (tarefa 1.0 do doc 03).
- **C. Askama mantido, com medição.** Os templates ficam numa crate própria, `elzevir-web-templates`, e o CI mede o tempo de recompilação incremental depois de mudar um template. Se passar de ~10 segundos, reavaliamos o MiniJinja para as telas.

## 10. Perfil da biblioteca de referência

A primeira biblioteca usuária (Biblioteca Municipal de Palotina) usa **MARC21**, classificação **CDD** e a **tabela Cutter PHA**. Esses três itens entram nos testes de aceitação desde a Fase 1 (importação, exibição, número de chamada e etiquetas).

## 11. Em aberto

- Como incluir os subdomínios gratuitos (§8).
- Demais funcionalidades do Koha Nexus, quando o repositório for compartilhado.
- Android como cliente (leitura de código de barras), depois da v1.
- Ainda há detalhes de planejamento a definir antes de começar a implementação.
