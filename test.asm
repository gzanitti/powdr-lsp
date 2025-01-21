machine Arith with
    degree: 8,
    latch: latch,
    operation_id: operation_id
{
    operation double<0> x -> y;
    operation square<1> x -> y;

    col witness operation_id;
    col fixed latch = [1]*;
    col fixed X(i) {i};
    col fixed DOUBLE(i) {2*i};
    col fixed SQUARE(i) {i*i};
    col witness x;
    col witness y;

    (1 - operation_id) $ [x, y] in [X, DOUBLE];
    operation_id $ [x, y] in [X, SQUARE];
}

machine Main with degree: 8 {
    Arith arith;

    reg pc[@pc];
    reg X[<=];
    reg Y[<=];
    reg A;

    instr double X -> Y link => Y = arith.double(X);
    instr square X -> Y link => Y = arith.square(X);
    instr assert_eq X, Y { X = Y }

    function main {
        A <== double(3);
        assert_eq A, 6;

        A <== square(3);
        assert_eq A, 9;
        return;
    }
}