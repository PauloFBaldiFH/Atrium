# Data de vencimento de um empréstimo

> Status: tarefa 1.12 (antes: v0 da tarefa 1.0). Base: Koha 26.05.05. Cobre prazo em dias e em horas, renovação, vencimento rígido e `ReturnBeforeExpiry`; o resto está em "Fora do escopo".

## 1. Comportamento

1. O prazo vem da regra de circulação efetiva para biblioteca × categoria do leitor × tipo de item: `issuelength`, ou `renewalperiod` na renovação quando ele está definido e não é vazio (`0` vale: vence no mesmo dia), e `lengthunit` (`days` ou `hours`).
2. O modo de contagem vem da regra `daysmode` do mesmo contexto; se ela não existe ou está vazia, vale a preferência `useDaysMode` (padrão do Koha: `Calendar`). Um valor desconhecido (por exemplo `noDays`) soma dias corridos, como `Days`, mas conta como "não `Days`" nas verificações de calendário dos itens 9 e 10.
3. O ponto de partida é o instante do empréstimo, no fuso da biblioteca. No empréstimo o vencimento é guardado sem segundos (`AddIssue`); na renovação a partir de "agora" os segundos ficam.
4. Prazo em **dias**: o vencimento é às **23:59** do dia calculado. Modos:
   - `Days`: soma os dias corridos e ignora o calendário.
   - `Calendar`: avança um dia aberto por vez, pulando cada dia fechado.
   - `Datedue`: soma os dias corridos; se o dia final está fechado, avança um dia por vez até um dia aberto.
   - `Dayweek`: soma os dias corridos; se o dia final está fechado, o primeiro passo é de 7 dias quando o prazo é múltiplo de 7 e o dia não é fechamento semanal, e de 1 dia nos outros casos. Os passos seguintes são de 7 dias sempre que o dia em que se está não é fechamento semanal, qualquer que seja o prazo.
5. Prazo em **horas**, `Days`: soma as horas. Com `ConsiderLibraryHoursInCirculation` em `close` ou `open`, e se o empréstimo passaria do fechamento de hoje, o vencimento é o fechamento de hoje (`close`) ou a abertura de amanhã (`open`).
6. Prazo em horas, outros modos: igual ao item 5 para `close`. Com `open`, parte da abertura no dia de hoje e soma 24 horas; sem conflito, soma as horas. Se o resultado cai num dia fechado, vai para o próximo dia aberto, às **10:00** (minutos mantidos).
7. O horário da biblioteca só conta se existem as linhas de hoje e de amanhã, com fechamento hoje e abertura amanhã; senão vale `ignore`.
8. **Vencimento rígido** (`hardduedate`, às 23:59 do dia): com `hardduedatecompare` `-1` limita (só se for antes), `0` fixa, `1` é piso (só se for depois).
9. **`ReturnBeforeExpiry`**: se o vencimento passa das 23:59 do dia de validade do cadastro (comparação de relógio, sem fuso), vira esse limite.
10. Depois do item 9, com modo diferente de `Days`, se o dia do vencimento está fechado ele recua para o dia aberto anterior.
11. Um dia está fechado quando, nesta ordem:
    1. há uma data avulsa para ele: feriado (fechado) ou exceção (aberto, mesmo que a regra semanal diga fechado);
    2. o dia da semana é fechado toda semana;
    3. o dia e mês são um feriado fixo anual.
12. A busca por dia aberto, para frente ou para trás, para depois de 5.000 passos e falha com "nenhum dia aberto".

## 2. Origem no Koha

- `C4::Circulation::CalcDateDue` e `AddIssue` (`reference/koha/C4/Circulation.pm`).
- `Koha::Calendar::addDuration`, `addDays`, `addHours`, `is_holiday`, `next_open_days`, `prev_open_days`, `get_push_amt` (`reference/koha/Koha/Calendar.pm`).
- `Koha::CirculationRules::get_effective_daysmode`.
- Testes: `reference/koha/t/db_dependent/Circulation/CalcDateDue.t`, `t/db_dependent/Koha/Calendar.t`.
- Telas: Ferramentas > Calendário (`tools/holidays.pl`); Administração > Bibliotecas (horário).

## 3. Paridade

`exigida`. Cenários:

- `scenarios/circulacao/vencimento-emprestimo-padrao.yaml`: empréstimo com a regra e o calendário reais da biblioteca (14 dias, `Datedue`), gravado no oráculo.
- `scenarios/circulacao/calcdatedue.t.yaml`: as 22 asserções de `CalcDateDue.t` (formato no doc 05, §4).

## 4. Casos de borda

1. Prazo de 0 dias: vence no mesmo dia, às 23:59.
2. `Datedue` com vencimento num sábado e biblioteca fechada aos sábados e domingos: vai para a segunda-feira.
3. `Datedue` com vencimento num feriado fixo (20/11): avança para o próximo dia aberto.
4. Exceção numa data que a regra semanal fecha: o dia conta como aberto.
5. `Calendar` com um fim de semana no meio do prazo: o vencimento anda dois dias.
6. `Dayweek`, 14 dias, vencimento num feriado avulso numa quinta: vai para a quinta seguinte.
7. `Dayweek`, 14 dias, vencimento num sábado fechado toda semana: avança 1 dia, não 7.
8. `Dayweek`, 10 dias, vencimento num domingo fechado toda semana e segunda feriado avulso: anda 1 dia até a segunda e depois 7, até a segunda seguinte.
9. Biblioteca fechada todos os dias: falha com "nenhum dia aberto".
10. `renewalperiod` vazio na renovação: usa `issuelength`.
11. `ReturnBeforeExpiry` com o dia da validade e o anterior fechados, modo `noDays`: recua dois dias.
12. Prazo de 3 horas às 14:00, biblioteca fecha às 16:00: `close` dá 16:00; `open` dá 10:00 do dia seguinte, ou do próximo dia aberto em modo `Calendar`.
13. Vencimento rígido antes, igual e depois do vencimento calculado, para cada `hardduedatecompare`.

## 5. Desvios

Nenhum. O passo de 7 dias do item 4 para prazos que não são múltiplos de 7 é comportamento do Koha (`get_push_amt` não olha o prazo) e está reproduzido.

## 6. Fora do escopo

- Vencimento escolhido à mão no balcão.
- Empréstimo de ILL (`ill_request->due_date`) e `decreaseLoanHighHolds`.
