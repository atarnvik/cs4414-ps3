PROGRAM_NAME = zhtta
TOOL_NAME = zhttanalyze

all: $(PROGRAM_NAME)
	 $(TOOL_NAME)

$(PROGRAM_NAME) : $(PROGRAM_NAME).rs gash.rs
	rustc $(PROGRAM_NAME).rs

$(TOOL_NAME) : $(TOOL_NAME).rs
	rustc $(TOOL_NAME).rs -L ../rust-http/build

clean :
	$(RM) $(PROGRAM_NAME)
	$(RM) $(TOOL_NAME)
    
run: ${PROGRAM_NAME}
	./${PROGRAM_NAME}

