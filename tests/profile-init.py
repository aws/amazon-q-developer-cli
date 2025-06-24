import sys
import time
import random
import subprocess


def profile_step(shell: str, id: str):
    timings = []
    last_timing = None
    child = subprocess.Popen(
        f"{shell} -x /tmp/{id}", shell=True, stderr=subprocess.PIPE
    )
    line = child.stderr.readline()
    while line != b"":
        if last_timing is None:
            last_timing = time.time_ns()
        new_timing = time.time_ns()
        timings.append((new_timing - last_timing, line))
        last_timing = new_timing
        line = child.stderr.readline()

    return timings


def test(shell: str, stage: str, cycles: int):
    id = random.randbytes(10).hex()
    print(f"testing {shell} {stage} with id {id}", file=sys.stderr)
    generate = subprocess.run(
        f"cargo run -p q_cli --release -- init {shell} {stage}",
        shell=True,
        stdout=subprocess.PIPE,
    )
    open(f"/tmp/{id}", "wb").write(generate.stdout)
    subprocess.run(f"chmod +x /tmp/{id}", shell=True)
    print("running shells", file=sys.stderr)
    runs = []
    for _ in range(cycles):
        runs.append(profile_step(shell, id))

    bad_runs = []
    pretty = []
    for num, run in enumerate(runs):
        for i in range(len(run)):
            if b"SHELL_PID" in run[i][1]:
                continue
            if run[i][1] != runs[0][i][1]:
                print(
                    f"discarding run {num} because {run[i][1]} != {runs[0][i][1]}",
                    file=sys.stderr,
                )
                bad_runs.append(num)

    if len(bad_runs) > cycles / 2:
        print("too many bad runs!")
        exit(1)

    for run in reversed(bad_runs):
        del runs[run]

    for i, line in enumerate(zip(*runs)):
        avg_timings = sum(map(lambda x: x[0], line)) / cycles
        code = line[0][1]
        pretty.append(
            (
                round(avg_timings / 1_000_000, 3),
                code.decode()[1 if shell == "zsh" else 2 : -1].replace(
                    f"/tmp/{id}", f"{shell}_{stage}"
                ),
            )
        )

    print("High impact lines:")
    for elapsed, line in filter(lambda x: float(x[0]) > 0.5, pretty):
        print(f"{elapsed}ms", line)

    print("-----")
    for elapsed, line in pretty:
        print(f"{elapsed}ms", line)


if len(sys.argv) < 3:
    print("usage: profile.init.py SHELL STAGE CYCLES")
    exit(1)

test(sys.argv[1], sys.argv[2], int(sys.argv[3]))
