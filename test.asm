use std::protocols::bus::bus_receive;
use std::protocols::bus::bus_send;
use std::prelude::Query;
use std::utils::force_bool;

let INTERACTION_ID = 1234;

mod types {
    enum Result<T> {
        Ok(T),
        Err
    }

    trait Arithmetic<T> {
        add: T, T -> T,
        mul: T, T -> T
    }
}

machine MainVM with 
    degree: 128,
{
    SubVM subvm(8, 64);
    Memory mem;

    reg pc[@pc];
    reg X[<=];
    reg Y[<=];
    reg Z[<=];
    reg A;
    reg B;
    reg CNT;
    reg ADDR;

    col witness XInv;
    col witness XIsZero;
    XIsZero = 1 - X * XInv;
    XIsZero * X = 0;
    XIsZero * (1 - XIsZero) = 0;

    col witness m_addr;
    col witness m_value;
    col witness m_is_write;
    col witness m_selector;
    force_bool(m_selector);

    col fixed operation_id = [0]*;
    col fixed latch = [0, 0, 0, 1]*;

    instr assert_zero X { XIsZero = 1 }
    instr mload -> X { [0, ADDR, m_value] is m_selector $ [m_is_write, m_addr, m_value] }
    instr mstore X { [1, ADDR, X] is m_selector $ [m_is_write, m_addr, m_value] }
    instr add X, Y -> Z { X + Y = Z }
    instr mul X, Y -> Z { X * Y = Z }
    instr square_and_double X -> Y, Z { Y = X * X, Z = 2 * X }
    instr call_subvm X, Y -> Z link => Z = subvm.compute(X, Y);
    
    bus_send(INTERACTION_ID, [0, X, Y, Z], m_is_write);

    function main {
        A <=X= ${ std::prelude::Query::Input(0, 1) };
        B <=Y= A - 7;
        
        ADDR <=X= 1;
        mstore(A);
        X <== mload();
        
        Y, Z <=Y,Z= square_and_double(X);
        
        A <== call_subvm(Y, Z);
        
        assert_zero A;
        return;
    }
}

machine SubVM {
    reg pc[@pc];
    reg X[<=];
    reg Y[<=];
    reg Z[<=];
    reg A;
    reg CNT;

    col fixed operation_id = [0]*;
    col witness XIsZero;
    
    instr add X, Y -> Z { X + Y = Z }
    instr mul X, Y -> Z { X * Y = Z }
    instr jmpz X, l: label { pc' = XIsZero * l + (1 - XIsZero) * (pc + 1) }
    instr jmp l: label { pc' = l }

    function compute x: field, y: field -> field {
        A <=X= x;
        CNT <=X= y;

        start:
        jmpz CNT, done;
        A <== mul(A, x);
        CNT <=X= CNT - 1;
        jmp start;

        done:
        return A;
    }
}

machine Memory {
    reg pc[@pc];
    reg ADDR;
    reg VAL;
    
    col witness mem_value;
    col witness mem_addr;
    
    instr write ADDR, VAL { mem_addr = ADDR, mem_value = VAL }
    instr read ADDR -> VAL { mem_addr = ADDR, VAL = mem_value }
}
