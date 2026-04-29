-------------------------
CLI Agent Harness specs
-------------------------
(last update: 24th April 2026)

= my desired ideal specs for a CLI designed to create/use AI Agents
(+ some requirements of things around it)
(then a UI can be built on top)

(mostly coming from the felt limitations with Shinkai, and from pain points with others like Hermes/codex/etc.)

My main pain points with modern stuff (Hermes Agent/Codex/etc.):
- opinionated defaults
- always-on defaults
- they do so much extra stuff, slower, less efficient, even when I don't need/want it (= harder to build simple agents without overhead)
- blackbox, not great visibility
- not fast/easy to configure
- Agents are not quite configurable enough
- resource intensive (storage, compute, tokens, time)

My main pain points with Shinkai: see documents linked below (the ones shared before about new features/vision)



# Model provider agnostic
-------------------------

- one function to call LLMs/Agents (independent of the underlying provider, like the shinkai-llm-prompt-processor)
- openAI-compatible

Rationale:
- easy to use
- a problem with the integration of a provider does not create issue over other providers
- providers/models integration can be updated independently
- allow for different LLM inference backends


# user configurable (LLM) models
-------------------------------

- while the CLI can set defaults, the user can declare the following for each LLM: max context length, max output tokens, available modalities, tool support, reasoning mode, default temperature,
- and any additional metadata: privacy level, cost tier, etc.

Rationale:
- users can integrate new models themselves
- users can organize/configure their models as they see fit


# Tools/skills calling modularity for AI Agents
-----------------------------------------------

- by default: allow multi-tool/skill calling, parallel and sequential
- but ability to set:
	-> zero tool call
	-> only one tool call allowed
	-> a max number of tool calls allowed (the LLM/Agent must know about it, to be directed to accomplish the task within the tool calls budget allowed, + indications when it's getting near the tool call limit)

Rationale:
- allowing for simple Agents
- ensure sequential executions of some steps
- guide Agents on complexity allowed to accomplish a task


# On/off LLM calls to interpret tool/skill results
-------------------------------------------------

- by default, the output of a tool/skill is allowed to be processed by a LLM/Agent
- but the user can declare both at the tool/skill level, and at the Agent level, for a given tool/skill or for all its tools/skills, if the output of a tool/skill must be interpreted by AI, or just delivered (mostly relevant for 1 tool calling configurations)
- additionally: for each tool/skill, possible to add a field 'output_interpretation_guidance', to activate/deactivate it (at tool/skill level and per agent), and to override it per agent (different agents can be guided to interpret the same tool/skill in different ways)


Rationale:
- reduce processing time/cost (AI interpretation of results has a cost)
- some tasks can be performed by AI but don't require an AI to interpret their final results
- seeing only an AI interpretation of a skill/tool output can be of lower quality than the original output
- use AI as a tool/skill trigger, and I read myself the result of the tool/skill (= AI chat as interface to trigger instead of clicks)
- easier integration of AI tasks in workflows/apps that require showing the result and not an interpretation of it
- easily built specialized routing AIs (picking tools/functions/actions among available ones, triggering and that's it, e.g. for games where you chat to act instead of clicking)
- being able to efficiently build separate processes/analyses/thoughts on top of same tools/skills


# LLM granularity per Agent
---------------------------

- being able to set different LLMs for different parts of an AI Agent
- by default: one LLM for the full Agent
- but possibility to set:
	-> a first LLM for prompt-to-tool/skill calling
	-> a second LLM for tool/skill interpretation
	-> a different LLM for interpretation of the output of a given tool/skill
- for a given skill/tool, I can define its associated LLM for output interpretation both at its tool/skill level, and at the agent level for this skill/tool (overriding the default from the skill/tool level)

Rationale:
- performance: narrow Agents with a few tools/skills can successfully call them with a small LLM (like FunctionGemma, a small qwen3.5/gemma4/ministral, etc.)
- interpreting tool/skill results might be of a different complexity level and of a different modality than just reacting to user prompts
- allow to pick a different LLM good at a specific task


# Sub-agents
------------

- the LLM/Agent prompt processing core feature is accessible by AI Agents, and usable inside tools/skills
- so AI Agents can call subagents
- so deterministic tools can be creates with LLM/Agents steps inside them (like in Shinkai)
- they act like tool calls if called by Agents (sequential/parallel, max number of calls, etc.)
- subagents executing tasks are observable (while a main agents is executing a task/loop, users can have visibility about what eventual subagents are doing)
- subagents can call subagents too
- the max depth of subagents usage is configurable


Rationale:
- more efficient agents if narrower tasks
- modularity
- easier to maintain
- integrate Agents/subagents steps in more deterministic workflows
- no black box




# performant configurable document ingestion
---------------------------------------------

- the default document ingestion feature must be very good, including with complex documents involving unusual layouts, tables, graphs, images, etc.
- document ingestion must support optional vision based ingestion
- the document ingestion feature/module can be easily changed for another one
- different document ingestion tools can be set up, and the user can configure which one to use per agents, per type of documents, per modality, etc.

Rationale:
- good document/information ingestion is the base of many tasks
- easily upgrade it as SOTA evolves
- optimize efficiency and cost of document ingestion



# accessibility
---------------

- the user can trigger common tasks/actions easily, so shortcuts (like typing '/+ something') to:
	-> trigger/enforce a tool/skill call directly
	-> trigger/enforce a tool/skill call directly with direct manual input filling (not AI filling the input, but really the user's inputs straight into tool calling (so not like Shinkai))
	-> access a specific agent
	-> use a saved prompt/commad: global library or library specific to a given agent (like an agent 'commands/prompts' list)


Rationale:
- fast tool calls with 100 % success on simple tool calls
- benefit from AI interpretation of tools/skills calling without first having an AI slowing or erroring on your tools/skills call
- Agents will be used many times in the same way, so faster if no need to retype prompts often used
- common commands for subagents can be powerful


# Highly configurable memory
----------------------------

- by default Agents have no memory of any kind (neither memory generation nor memory loading)

## memory generation (the act of creating/saving memories/facts/preferences/rules, etc.)
-----
- memory generation is activable per Agent
- memory generation is activable per task/topic per Agent (= I can declare tasks/topics per Agent for which memory creation will be authorized = if these tasks/topics not present -> no memory processing)
- memory generation is activable/removable at conversation level
- memory generation is non-blocking/parallel (when memory is processed in real time, during Agent usage, user can still use the Agent meanwhile)
- memory generation can be set to be asynchronous or manual (= process it at a given cron/time, or a command to start its process manually)
- memory generation can be focused on part of the conversations (user can select a section of conversation, and ask for memory generation only on that part)
- different memory systems/SDK/frameworks/tools, etc. can be supported (it's something processing on top of the conversation history, and integrating into context at LLM call building step)
- each memory systems/SDK/frameworks/tools can use a distinct LLM, this LLM can be overridden per Agent


Rationale:
- an Agent with a memory is a specific use case, not mandatory for all tasks/deployments
- users might not always need the same memory behaviour from a same agent
- efficiency: memory adds tokens costs, processing time, disk storage, etc.
- speed: don't mandatorily have the device process memories while I do other things
- support personal/companion like Agents, with memory of users
- support self-evolving Agents


## memory inclusion (the act of loading memories in the conversation)
-----
- memory loading can be activated/deactivated at agent level (even separately from memory generation being activated or not)
- memory loading can be activated/deactivated  at the conversation level (even separately from memory generation being activated or not)
- memory loading can be activated/deactivated per topic

Rationale:
- user might not always want the Agent to use its memories, or not its full memories


## configurable accessibility of memory
-----
- by default, memories are restricted to the Agent generating them
- memory can be generated by/accessed by multiple agents/profiles the user declare (=several user profiles/agents can benefit from common memory, or from memory of another agent)
- memory can be exported/imported from/to an Agent

Rationale:
- modular agents/subagents builds
- sharing, team work


## manually editable memory
-----
- memories must be in a format easy to read and manually edit for a human
- memories must have version control, at least a 1-2 cycles/steps reroll capability (= reset a memory to its 1-2 previous state)

Rationale:
- user easily able to correct memories if LLMs/Agents generate faulty ones
- user easily able to configure/add correct memories (as part of Agent configuration, or at any time)
- able to create agents/subagents with pre-defined starting memories (e.g., good for interactive experiences ?)



# Configurable tools/skills loading
-----------------------------------

- how much information an Agent receives about its available tools/skills list must be highly configurable
- possible behaviours :
	-> see full lists with full visibility of tools/skills names, descriptions, parameters
	-> see only the names, then loads more details if considered or selected for use
	-> see only names and descriptions, then loads parameters details only when selected to use
- loading tools/skills details is configurable at different levels:
	-> by default at the tools/skills level
	-> at the Agent level per tool/skill (overriding the setting at tool/skill level)

Rationale:
- able to build faster agents with full details on available tools (which might require strong LLMs)
- able to build agents with many tools/skills accessible, without them making too many mistakes because of context bloat
- able to use smaller LLMs because tools/skills visibility + calling is gradual
- able to build agents with main tools/skills (always well visible) and secondary tools/skills (visible by their names only and fetching details in the rarer moments they'll be needed)


# Team work / Profiles
----------------------

- One main profile.
- Default = main profile = All configs are set at the main profile level (= agents, tools/skills, prompt library, memories, etc. -> all go to a main profile by default)

- Ability to create different profiles
- Everything can be allowed access by a different profile (= user can declare agents they have in one profile to be available in another profile ; memory of a given agent can be generated/accessed within the main profile or any other profiles the user declare ; tool/skills are available to any profiles the user declares ; etc.)
- Everything can be given access to a different profile (= user can declare that an Agent is allowed to use the tool/skills/memories of another (agent in a different) profile (pending auth from this profile)

Rationale:
- allow one user easy navigation to a set of agents, a set of usage, etc.
- allow multi-user
- allow easy sharing
- allow team work


# observable modular context construction
----------------------------------------

Given the above specs, the context sent to a LLM call at any given time is highly modular:
- inclusion or not of tools/skills list more detailed info
- inclusion or not of memories
- compacted context
- profile provenance of some parts of the context
- tools/skills calls numbers
- etc.

- this 'at LLM-call' context input must be accessible/visible for the user
= for each prompt typed (even before it's sent), the user can trigger something to see the full context around it that is actually send to the LLM (anything from memory, rules, tool lists, etc.)

Rationale:
- the user must be able to see what is actually happening per LLM call (no black box)
- user can check the prompt building/context part of the Agent is configured as intended, user can identify parts that might be wrong, can optimize
- user can better get a sense of tokens costs for each Agent usage


# configurable context compaction
---------------------------------

- context compaction = keep the most important parts/details of the conversation so far (the full conversation history is not sent to the next LLM call, just the new compacted context + the next prompt)
- context compaction can be guided = the user can define what elements to keep, what to loose (at agent level, at conversation level, at manual trigger)
- context compaction has a transferable/portable format : the user can easily export the context to another conversation, save it somewhere, transfer it to another profile, etc.
- context compaction can be manually triggered at any time in a conversation
- context compaction has a configurable max tokens count per conversation before context compaction
- context compaction has a max length to aim for the output of the compaction(token/word/paragraph count ?)
- context compaction parameters can be set at default global level, at agent level, at conversation level, at manual trigger level (each overriding the one before)
- context compaction is well compatible with branching : in case the user will branch at an earlier conversation point, the full original conversation must still be accessible and sendable to a LLM/Agent (but because the full conversation still has to be displayed for users to see, it's still saved somewhere so accessible)

Rationale:
- manage tokens cost
- allow users to pursue long conversations with flexibility of information retention
- allow efficient re-use and sharing of conversations outcomes


# tools/skills organisation (sets/packs/categories)
---------------------------------------------------

- tools/skills can belong to one or several categorie(s)
- full categories can be activated/deactivated at full default level
- full categories can be activated/deactivated at Agent level
- full categories can be activated/deactivated at conversation level
- full categories can be given access to another profile

Rationale:
- easy management of AI capabilities at all levels


# configurable deleting of conversations
----------------------------------------

- conversations can be fully deleted (actually removing everything related to the conversation from the device, including any metadata around it, eventual memories, compacted context, etc.)
- a set of selected messages can be similarly deleted
- when deleting conversations/selected messages, users can:
	-> choose to keep: generated files (or even which files ?),
	-> trigger context compaction first, and keep just this compacted context (the context compaction can be guided)
	-> trigger memory generation first, and keep just these memories (memory creation can be guided)
	-> choose a combination of the above
- deleting multiple conversations at once is possible and easy (e.g. : selected conversations, all the conversations with a selected Agent/set of agents)


Rationale:
- users must be able to manage/limit the disk storage size of their agentic systems (which can actually blow-up, e.g., see how many times I complained about this for Shinkai)



# configurable automatic prompt refinement per Agent
----------------------------------------------------

- per Agent, users can configure a Agent/LLM and instructions to use to pre-process user's prompts to improve them before actually making the Agent/LLM call (the actual agent will see only the improved prompt, not the original one)
- such improvements instructions can multiple per agent: different ones defined for different kind of topics/tasks, etc.
- an Agent can be aware of the prompt improvements instructions it has, so that it can guide users to better prompt (but not mandatory)

Rationale:
- help obtain better results for external users of the created agents (when the agent creator is not the one using it)
- allow to quickly use agents, by not needing to carefully prompt




# branching conversations
---------------------------------------------------

- conversations with the Agents/LLMs can be branched at any point
- branches can be independently deleted without deleting the main branch (it deletes up to the last branching point)
- the main branch (the part above any branching) is not replicated on disk
- branches can be well visualized and navigated too (some kind of tree view (3D ?) where the user see main topics, different branching points with the starting divergence/topic/reason/idea of each branch)


Rationale:
- allowing exploring different aspects, or retrying without context bloat
- there is nearly no point having branching if it's not easy to navigate, go back and forth between the different sides of the conversation explored, or to delete (=ignore well) what became less interesting/irrelevant/meaningless, etc.
- minimum usage of disk space


# stopping processes with/without context of stopped task(s)
---------------------------------------------------------

- all the following can be stopped at any time: tools/skills processes, tools/skills calls loops, Agent thoughts processes, LLM inference
- context retention from the stopped tasked is configurable (at global level, and at agent level):
	-> on = the Agent (or a separate LLM call ?) aggregates/summarize what was done/attempted/obtained, seemed to bug, etc. during the stop tasked (delivers just this, without any reaction to it)
	-> off = zero context of what was last running is added, the conversation history is back to the last user prompt (which can be deleted easily, to make sure no trigger again the same fail)
- a stopped task/loop can be restarted from a selected step if its history was saved

Rationale:
- safety to stop something going wrong
- save time/cost on wrong directions
- save time/cost when developing/testing agents, support more efficient testing
- supporting both self-evolving agent that can learn/adapt from fails, and standard agents only defined by users



# guiding Agents during a run
-----------------------------

- ability to add a message/prompt to the Agent/LLM without stopping its current task/loop nor waiting for it to finish
(= user-message injection that lands between tool-call iterations without interrupting the agent or creating a new user turn)

Rationale:
- support course-correct an agent in-flight




# tokens and time cost observability
-----------------------------------

- for each Agent/LLM answer/loop, the user can access info on tokens count (in and out, and its associated cost) and processing time
- for any part of the conversation, the user can access info on global tokens count+cost and processing time (adding all the counts of each answer) (e.g., from the start of the conversation, for the last n messages, for a selected part of the conversation)
- user can manually define the in/out token cost per LLM

Rationale:
- allow users to assess money/time costs (apply to both personal usage or using/selling Agents in work context), and to optimize
- support evaluations, including building evaluations/optimizations suites/protocols on top


# manual quality assessment of Agents/LLMs answers
--------------------------------------------------

- shortcut/feature to declare the quality of a given Agent/LLM answer, loop, etc. (e.g., give a n/10 mark) = a quality score gets assigned to that specific answer
- same at full conversation level / at selected part of the conversation level

Rationale:
- users can bookmark and easy re-find high quality answers in a conversation or across them, or also poor answers
- helps building evaluations on top
- helps support self-evolving agents


# configurable Agents/LLMs possibility to create agents/tools/skills
-------------------------------------------------------------------

- agents/tools/skills creation is not limited to users, Agents and LLMs can access it
- agens can both create new tools/skills, and discover/add new tools/skills from available sources (your own saved tools/skills, or from an external tool/skill repository)
- agents/tools/skills creation access it activable/de-activable at each level: global, profile, agent, conversation
- agents/tools/skills creation by Agents/LLMs can be guided (e.g., instructions how to do so for what when activating the feature)
- agents can use the tools/skills they've just created
- agents can use as subagents (= as tools) the agents they've just created
- created agents/subagents by agents can be ephemeral/deleted or saved by the user to be available again elsewhere

Rationale:
- support AI assisted agent creation, optimization, etc.
- self-evolving Agents building specialized modular components of their subagents systems
- allow to build interactive experiences with spawning agents (e.g., Shinkai DnD demo I made)


# configurable prompt injection guardrails
-----------------------------------------

- per agents, a prompt injection guardrail can be defined for when they read documents, access online material, etc.
= a separate LLM call (not the original agent) is made to evaluate prompt injection risk of the material, and warn agent/user if risk or proceed normally if no issue (without giving any details about this assessment to the original agent, to avoid adding to context)

Rationale:
- allow to create/use safer agents exposed to outside material



# human in the loop
-------------------

- the user can configure human-in-the-loop mandatory steps for certain tools/skills calls, for certain folder/file read/write access
- could even be coupled with the possibility to configure external controller agents (like a local agent configured to assess authorization levels, accept/deny steps or certain actions)
- password/signature support : allow to actual block some actions until a password is typed / a cryptographic signature is made

Rationale:
- allow human/AI control/validation where wanted
- security : guarantee control by a specific (set of) human(s) (not just someone in front of the device)


#List/bulk/loop mode
---------------

- a agent/LLM task can be easily applied to a list instead of a single item (e.g., on all files in a folder, on a set of prompts instead of one) with 100 % guarantee that it will get done for each (like not a LLM making a plan of tasks, but actual deterministic list/bulk/loop execution)

Rationale:
- support bulk tasks


# exportable/importable everything
------------------------------------

- users can export/import fully, in 1 clik/command, all the following: profile configuration, full agent configuration, a tool/skill, a LLM config, etc.

Rationale:
- easy back-ups outside of the CLI managed folders
- portability, sharing



# configurable voice mode
-------------------------

- voice input: agents/LLMs can be interacted with by voice input
- voice output: agents/LLMs answers can be audio played
- voice mode Text-to-speech is configurable at default global level and at agent level (provider/model used, voice choice, tone, etc.)
- support both local and cloud based


Rationale:
- typeless is a thing for many users
- part of audio support for agents use from a mobile



# agents are accessible from mobile and common platforms
--------------------------------------------------------

- support at least one common messaging platform to access your Agents from mobile, etc.
- support at least one common messaging platform to access your Agents in a work context

(e.g., Hermes Agent is currently at 17 platforms supported, like Telegram, Slack, Teams, etc.)

- support calling agents from anywhere
- support embedding agents chat in anywhere


Rationale:
- people use their Agents also (mostly in the future ?) when not in front of a computer
- don't ask user to install yet another new something on their mobile
- users need their agents inside the stack they already use
- builders can create AI experiences of all kinds served anywhere with agents built with this CLI





========================================================================================================
Below : important requirements,
but that might be external to core CLIs, possibly as tools/skills, or releted to interface on top of it
========================================================================================================

# running/executing code
------------------------

- agents/LLM must be able to trigger code snippets, at least CLI/command like, python, typescript
- with guardrails
- but some (most ?) of it can be packaged as tools/skills

Rationale:
- having agents able to perform new tasks that were not already coded
- having agents able to navigate better, to better use what is available



# agents with payments capabilities
------------------------------------

- agents must be able to pay for service they access (e.g., x402 support)
- agents should be able to be paid for services they provide
- spend limits must be configurable
- wallet access configurable
- could be tools/skills ? (and not core ?)

Rationale:
- support a future where agentic AI is really a thing across the web
(- with this comes identity / discoverability / guarantees / auditability / verifiability issues, which extend the scope considerably)



# generating documents
-----------------------

- there must be good default tools for generation of documents of common formats (e.g., word, pdf, excel/csv, slide/powerpoint, etc.)
- could be tools/skills ? (and not core ?)

Rationale:
- many tasks require such outputs
- users copy pasting stuff around is outdated


# supporting different documents formats view
--------------------------------------------

- the interface must be able to show in-chat the content of common formats (e.g., images, play audio, pdfs, csv)
- max 1 click view
- or at least 1 click open on device in default app

Rationale:
- easily see the output of your AI systems








===================================================
Sidenote: previous documents
===================================================

Can also refer to these 2 previous documents about upgrading Shinkai:

- exploring a vision for 2026:
https://docs.google.com/document/d/1oKub7TykYZCly48gDbPPYpoyGumw4P7TgkFv-YR4qUk/edit?usp=drive_link
(last update: January 20th, 2026)

- possible new features:
https://docs.google.com/document/d/1ouQC7hLFW819Zx1Pl3Pae4GYxIn8GicfymbrY4l6oYw/edit?usp=drive_link
(last update: January 20th, 2026)
