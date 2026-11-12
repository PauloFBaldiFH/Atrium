# Data de vencimento de um empréstimo (v0)

> Status: v0, tarefa 1.0 (primeiro cenário do executor). Base: Koha 26.05.05. Cobre o empréstimo com prazo em **dias**; o resto está em "Fora do escopo".

## 1. Comportamento

1. O prazo vem da regra de circulação efetiva para biblioteca × categoria do leitor × tipo de item: `issuelength` (ou `renewalperiod` na renovação, quando definido e não vazio) e `lengthunit`.
2. O modo de contagem vem da regra `daysmode` do mesmo contexto; se ela não existe ou está vazia, vale a preferência `useDaysMode` (padrão do Koha: `Calendar`).
3. O ponto de partida é o instante do empréstimo, truncado ao minuto, no fuso da biblioteca.
4. Com prazo em dias, o vencimento é sempre às **23:59** do dia calculado.
5. Modos de contagem:
   - `Days`: soma os dias corridos e ignora o calendário.
   - `Calendar`: avança um dia aberto por vez, pulando cada dia fechado.
   - `Datedue`: soma os dias corridos; se o dia final está fechado, avança para o próximo dia aberto.
   - `Dayweek`: como `Datedue`, mas se o prazo é múltiplo de 7 e o dia final fechado não é um fechamento semanal, avança de 7 em 7 dias (mantém o dia da semana) até achar um dia aberto.
6. Um dia está fechado quando, nesta ordem:
   1. há uma data avulsa para ele: feriado (fechado) ou exceção (aberto, mesmo que a regra semanal diga fechado);
   2. o dia da semana é fechado toda semana;
   3. o dia e mês são um feriado fixo anual.
7. A busca por dia aberto para depois de 5.000 passos e falha com "nenhum dia aberto".

## 2. Origem no Koha

- `C4::Circulation::CalcDateDue` (`reference/koha/C4/Circulation.pm`).
- `Koha::Calendar::addDuration`, `addDays`, `is_holiday`, `next_open_days`, `prev_open_days`, `get_push_amt` (`reference/koha/Koha/Calendar.pm`).
- `Koha::CirculationRules::get_effective_daysmode`.
- Testes: `reference/koha/t/db_dependent/Circulation/CalcDateDue.t`, `t/db_dependent/Koha/Calendar.t`.
- Tela: Ferramentas > Calendário (`tools/holidays.pl`).

## 3. Paridade

`exigida`. Cenários:

- `scenarios/circulacao/vencimento-emprestimo-padrao.yaml`: empréstimo com a regra e o calendário reais da biblioteca (14 dias, `Datedue`).

## 4. Casos de borda

1. Prazo de 0 dias: vence no mesmo dia, às 23:59.
2. `Datedue` com vencimento num sábado e biblioteca fechada aos sábados e domingos: vai para a segunda-feira.
3. `Datedue` com vencimento num feriado fixo (20/11): avança para o próximo dia aberto.
4. Exceção numa data que a regra semanal fecha: o dia conta como aberto.
5. `Calendar` com um fim de semana no meio do prazo: o vencimento anda dois dias.
6. `Dayweek`, 14 dias, vencimento num feriado avulso numa quinta: vai para a quinta seguinte.
7. `Dayweek`, 14 dias, vencimento num sábado fechado toda semana: avança 1 dia, não 7.
8. Biblioteca fechada todos os dias: falha com "nenhum dia aberto".

## 5. Desvios

Nenhum.

## 6. Fora do escopo (v0)

- Prazo em horas (`lengthunit = hours`) e a preferência `ConsiderLibraryHoursInCirculation`.
- Data de vencimento rígida (`hardduedate`, `hardduedatecompare`).
- `ReturnBeforeExpiry` (vencimento limitado à validade do cadastro do leitor).
- Vencimento escolhido à mão no balcão.
